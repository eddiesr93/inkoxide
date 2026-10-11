/** `<Image src>` from files, request objects, functions and promises, loaded before the engine. */
import { readFileSync } from 'node:fs';
import { createServer, type IncomingMessage } from 'node:http';
import type { AddressInfo } from 'node:net';
import { join, relative, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { z } from 'zod';

import {
  countPages,
  Document,
  generate,
  type ImageSrc,
  Image,
  Page,
  RenderError,
  type RenderToBufferOptions,
  renderToBuffer,
} from '../src/index.ts';

const logoPath = resolve(import.meta.dirname, '../../../fixtures/images/logo.png');
const logo = readFileSync(logoPath);

const Doc = ({ src }: { src: ImageSrc }) => (
  <Document>
    <Page>
      <Image src={src} className="h-10 w-10" />
    </Page>
  </Document>
);

const render = (src: ImageSrc, opts?: RenderToBufferOptions) => renderToBuffer(<Doc src={src} />, opts);
const failure = (src: ImageSrc, opts?: RenderToBufferOptions): Promise<RenderError> =>
  render(src, opts).then(
    () => {
      throw new Error('expected the render to fail');
    },
    (e: unknown) => {
      if (e instanceof RenderError) return e;
      throw e;
    },
  );

let server: ReturnType<typeof createServer>;
let base: string;
const seen: Array<{ method?: string; header?: string | string[]; body: string }> = [];

const bodyOf = async (req: IncomingMessage): Promise<string> => {
  const chunks: Buffer[] = [];
  for await (const c of req) chunks.push(c as Buffer);
  return Buffer.concat(chunks).toString('utf8');
};

beforeAll(async () => {
  server = createServer(async (req, res) => {
    seen.push({ method: req.method, header: req.headers['x-token'], body: await bodyOf(req) });
    if (req.url === '/logo.png') res.end(logo);
    else if (req.url === '/moved') res.writeHead(302, { location: '/logo.png' }).end();
    else res.writeHead(404).end();
  });
  await new Promise<void>((ok) => server.listen(0, '127.0.0.1', ok));
  base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});

afterAll(() => new Promise<void>((ok) => server.close(() => ok())));

const allowed = (): RenderToBufferOptions => ({ remoteImages: { allow: [`${base}/`] } });

describe('image files', () => {
  it('reads an absolute path, a path relative to the working directory and a file: URL', async () => {
    const rel = relative(process.cwd(), logoPath);
    for (const src of [logoPath, rel.startsWith('.') ? rel : `./${rel}`, pathToFileURL(logoPath).href]) {
      expect(countPages(await render(src))).toBe(1);
    }
  });

  it('names the path, and where it resolved to, when a file cannot be read', async () => {
    const err = await failure('./no/such/logo.png');
    expect(err.code).toBe('IMAGE_READ_FAILED');
    expect(err.status).toBe(400);
    expect(err.message).toBe(
      `Cannot read the image ./no/such/logo.png (${resolve('no/such/logo.png')}): no such file.`,
    );
    const dir = await failure(join(logoPath, '..'));
    expect(dir.message).toBe(`Cannot read the image ${join(logoPath, '..')}: not a file.`);
  });

  it('uses a passed-in image whose id looks like a path, without reading a file', async () => {
    const pdf = await render('./no/such/logo.png', { images: { './no/such/logo.png': logo } });
    expect(countPages(pdf)).toBe(1);
  });

  it('keeps a bare name as an image id', async () => {
    expect(countPages(await render('logo.png', { images: { 'logo.png': logo } }))).toBe(1);
    const err = await failure('logo.png');
    expect(err.code).toBe('ENGINE_REJECTED');
  });
});

describe('image requests', () => {
  it('sends the method, headers and body to an allowed uri', async () => {
    seen.length = 0;
    const src = {
      uri: `${base}/logo.png`,
      method: 'POST',
      headers: { 'x-token': 't0k' },
      body: '{"size":40}',
    };
    expect(countPages(await render(src, allowed()))).toBe(1);
    expect(seen).toEqual([{ method: 'POST', header: 't0k', body: '{"size":40}' }]);
  });

  it('refuses a uri outside the allowlist, redirects, and errors with the uri', async () => {
    const outside = await failure({ uri: 'https://example.com/logo.png' }, allowed());
    expect(outside.code).toBe('TEMPLATE_ERROR');
    expect(outside.message).toContain('remoteImages allowlist');
    const moved = await failure({ uri: `${base}/moved` }, allowed());
    expect(moved.code).toBe('IMAGE_FETCH_FAILED');
    expect((await failure({ uri: `${base}/gone.png` }, allowed())).message).toBe(
      `Cannot fetch the image ${base}/gone.png: HTTP 404.`,
    );
  });

  it('applies the size limit', async () => {
    const err = await failure(
      { uri: `${base}/logo.png` },
      { remoteImages: { allow: [`${base}/`], maxBytes: 10 } },
    );
    expect(err.code).toBe('IMAGE_FETCH_FAILED');
    expect(err.message).toContain('over the 10-byte limit');
  });
});

describe('image functions and promises', () => {
  it('resolves to bytes, a path, a passed-in id, a data: URL or a request', async () => {
    const dataUrl = `data:image/png;base64,${logo.toString('base64')}`;
    const sources: ImageSrc[] = [
      () => logo,
      Promise.resolve(logo),
      async () => logoPath,
      () => 'brand',
      Promise.resolve(dataUrl),
      async () => ({ uri: `${base}/logo.png` }),
      () => `${base}/logo.png`,
    ];
    for (const src of sources) {
      expect(countPages(await render(src, { ...allowed(), images: { brand: logo } }))).toBe(1);
    }
  });

  it('calls a function once per render, however many images use it', async () => {
    let calls = 0;
    const src = () => {
      calls++;
      return logo;
    };
    await renderToBuffer(
      <Document>
        <Page>
          <Image src={src} />
          <Image src={src} />
        </Page>
      </Document>,
    );
    expect(calls).toBe(1);
  });

  it('fails the render when the function throws or the promise rejects', async () => {
    const thrown = await failure(() => {
      throw new Error('logo service down');
    });
    expect(thrown.code).toBe('TEMPLATE_ERROR');
    expect(thrown.message).toBe('An <Image src> function or promise failed: logo service down');
    const rejected = await failure(Promise.reject(new Error('no logo')));
    expect(rejected.message).toBe('An <Image src> function or promise failed: no logo');
  });

  it('checks what it resolves to like a src written in the template', async () => {
    expect((await failure(() => 'brand')).message).toBe(
      'An <Image src> function or promise resolved to "brand", which is not an id in the images option, a path or a URL.',
    );
    expect((await failure(() => './no/such/logo.png')).code).toBe('IMAGE_READ_FAILED');
    expect((await failure(async () => 'https://example.com/logo.png', allowed())).message).toContain(
      'remoteImages allowlist',
    );
    expect((await failure((() => 42) as unknown as ImageSrc)).message).toBe(
      'An <Image src> function or promise resolved to number; expected bytes, a string or a request.',
    );
  });
});

describe('generate', () => {
  it('reads files and lets template images win over paths', async () => {
    const template = {
      meta: { id: 'images' },
      schema: z.object({}),
      Template: () => (
        <Document>
          <Page>
            <Image src={logoPath} className="h-10 w-10" />
            <Image src="./brand.png" className="h-10 w-10" />
          </Page>
        </Document>
      ),
      images: { './brand.png': logo },
    };
    const r = await generate(template, {});
    expect(r.pages).toBe(1);
  });
});
