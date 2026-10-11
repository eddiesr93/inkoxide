/** `<Image src>` from files, request objects, functions and promises, loaded before the engine. */
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { createServer, type IncomingMessage } from 'node:http';
import type { AddressInfo } from 'node:net';
import { tmpdir } from 'node:os';
import { delimiter, dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { z } from 'zod';

import {
  countPages,
  Document,
  generate,
  Header,
  type ImageSrc,
  Image,
  type LocalImageOptions,
  Page,
  PageBreak,
  RenderError,
  renderSettled,
  type RenderToBufferOptions,
  renderToBuffer,
  renderTree,
  Text,
  View,
} from '../src/index.ts';

const fixtures = resolve(import.meta.dirname, '../../../fixtures/images');
const logoPath = join(fixtures, 'logo.png');
const logo = readFileSync(logoPath);
const palette = readFileSync(join(fixtures, 'palette.png'));
const photo = readFileSync(join(fixtures, 'photo.jpg'));
// Their sizes, as the PDF lists them.
const LOGO = '160x160';
const PALETTE = '48x32';
const PHOTO = '320x200';

/** The sizes of the images each page of an uncompressed PDF draws, page by page. */
function pageImages(pdf: Uint8Array): string[][] {
  const text = Buffer.from(pdf).toString('latin1');
  const objects = new Map<string, string>();
  for (const m of text.matchAll(/(\d+) 0 obj([\s\S]*?)(?:stream|endobj)/g))
    objects.set(m[1] ?? '', m[2] ?? '');
  const pages = [...objects.values()].find((o) => /\/Type \/Pages\b/.test(o)) ?? '';
  const kids = /\/Kids \[([^\]]*)\]/.exec(pages)?.[1] ?? '';
  return [...kids.matchAll(/(\d+) 0 R/g)].map(([, page]) => {
    const xobjects = /\/XObject <<([^>]*)>>/.exec(objects.get(page ?? '') ?? '')?.[1] ?? '';
    return [...xobjects.matchAll(/(\d+) 0 R/g)].map(([, id]) => {
      const o = objects.get(id ?? '') ?? '';
      return `${/\/Width (\d+)/.exec(o)?.[1]}x${/\/Height (\d+)/.exec(o)?.[1]}`;
    });
  });
}

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
    const body = seen[seen.length - 1]?.body;
    if (req.url === '/logo.png') res.end(logo);
    else if (req.url === '/palette.png') res.end(palette);
    else if (req.url === '/photo.jpg') res.end(photo);
    else if (req.url === '/image' && req.method === 'POST') res.end(body === 'palette' ? palette : logo);
    else if (req.url === '/moved') res.writeHead(302, { location: '/logo.png' }).end();
    else res.writeHead(404).end();
  });
  await new Promise<void>((ok) => server.listen(0, '127.0.0.1', ok));
  base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});

afterAll(() => new Promise<void>((ok) => server.close(() => ok())));

const allowed = (): RenderToBufferOptions => ({ remoteImages: { allow: [`${base}/`] } });

describe('image files', () => {
  // root/{logo.png, sub/logo.png, fifo, out.png -> outside/secret.png}, other/logo.png
  let dir: string;
  let root: string;
  let other: string;
  let secret: string;
  const local = (extra?: Partial<LocalImageOptions>): RenderToBufferOptions => ({
    localImages: { root, ...extra },
  });

  beforeAll(() => {
    dir = mkdtempSync(join(tmpdir(), 'inkoxide-images-'));
    root = join(dir, 'root');
    other = join(dir, 'other');
    mkdirSync(join(root, 'sub'), { recursive: true });
    mkdirSync(other);
    mkdirSync(join(dir, 'outside'));
    secret = join(dir, 'outside', 'secret.png');
    for (const f of [join(root, 'logo.png'), join(root, 'sub', 'logo.png'), join(other, 'logo.png'), secret])
      writeFileSync(f, logo);
    symlinkSync(secret, join(root, 'out.png'));
    if (process.platform !== 'win32') execFileSync('mkfifo', [join(root, 'fifo')]);
  });

  afterAll(() => rmSync(dir, { recursive: true, force: true }));

  it('reads no file unless local images are allowed: a path is an image id', async () => {
    for (const src of ['./logo.png', logoPath, pathToFileURL(logoPath).href]) {
      const err = await failure(src);
      expect(err.code).toBe(src.startsWith('file:') ? 'TEMPLATE_ERROR' : 'ENGINE_REJECTED');
      if (!src.startsWith('file:')) expect(err.engineCode).toBe('UNKNOWN_IMAGE');
    }
    expect((await failure(() => logoPath)).message).toBe(
      `<Image src="${logoPath}">: image files are off. Allow their folder with the render option localImages.root (or the INKOXIDE_LOCAL_IMAGES variable), or pass the bytes.`,
    );
  });

  it('reads a path relative to the root, an absolute path and a file: URL inside it', async () => {
    for (const src of [
      './logo.png',
      './sub/../logo.png',
      './sub/logo.png',
      join(root, 'logo.png'),
      pathToFileURL(join(root, 'sub', 'logo.png')).href,
      () => './sub/logo.png',
    ]) {
      expect(countPages(await render(src, local()))).toBe(1);
    }
  });

  it('takes several roots: relative paths resolve against the first', async () => {
    const roots = { localImages: { root: [root, other] } };
    expect(countPages(await render(join(other, 'logo.png'), roots))).toBe(1);
    expect(countPages(await render('./logo.png', roots))).toBe(1);
  });

  it('reads the root from INKOXIDE_LOCAL_IMAGES', async () => {
    process.env.INKOXIDE_LOCAL_IMAGES = `${other}${delimiter}${root}`;
    try {
      expect(countPages(await render(join(root, 'sub', 'logo.png')))).toBe(1);
    } finally {
      delete process.env.INKOXIDE_LOCAL_IMAGES;
    }
  });

  it('refuses paths outside the root, symbolic links included, with the error of a missing file', async () => {
    const outside = [
      '../outside/secret.png',
      './sub/../../outside/secret.png',
      secret,
      pathToFileURL(secret).href,
      './out.png',
      () => '../outside/secret.png',
    ];
    for (const src of [...outside, './missing.png', '../outside/missing.png']) {
      const err = await failure(src, local());
      expect(err.code).toBe('IMAGE_READ_FAILED');
      const written = typeof src === 'function' ? src() : src;
      expect(err.message).toBe(`Cannot read the image ${written}: no such file in the localImages folders.`);
      expect(err.cause).toBeUndefined();
    }
  });

  it('refuses network paths without touching the network', async () => {
    for (const src of ['\\\\host\\share\\a.png', '//host/share/a.png', 'file://host/share/a.png']) {
      const err = await failure(src, local());
      expect(err.code).toBe('IMAGE_READ_FAILED');
      expect(err.message).toBe(`Cannot read the image ${src}: network paths are not allowed.`);
    }
  });

  it('applies the size limit before reading', async () => {
    const err = await failure('./logo.png', local({ maxBytes: 10 }));
    expect(err.message).toBe(
      `Cannot read the image ./logo.png: ${logo.byteLength} bytes, over the 10-byte limit.`,
    );
  });

  it('reads regular files only, and does not wait on a FIFO', async () => {
    expect((await failure('./sub', local())).message).toBe('Cannot read the image ./sub: not a file.');
    if (process.platform !== 'win32') {
      expect((await failure('./fifo', local())).message).toBe('Cannot read the image ./fifo: not a file.');
    }
  });

  it('never names where a path resolved to', async () => {
    for (const src of ['./missing.png', '../outside/secret.png', './sub', './out.png']) {
      const err = await failure(src, local());
      expect(err.message).not.toContain(dir);
      expect(JSON.stringify(err)).not.toContain(dir);
    }
  });

  it('uses a passed-in image whose id looks like a path, without reading a file', async () => {
    const pdf = await render('./no/such/logo.png', { ...local(), images: { './no/such/logo.png': logo } });
    expect(countPages(pdf)).toBe(1);
  });

  it('keeps a bare name as an image id', async () => {
    expect(countPages(await render('logo.png', { ...local(), images: { 'logo.png': logo } }))).toBe(1);
    const err = await failure('logo.png', local());
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
      const opts = { ...allowed(), localImages: { root: dirname(logoPath) }, images: { brand: logo } };
      expect(countPages(await render(src, opts))).toBe(1);
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
    const files = { localImages: { root: dirname(logoPath) } };
    expect((await failure(() => './no/such/logo.png', files)).code).toBe('IMAGE_READ_FAILED');
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
    const r = await generate(template, {}, { localImages: { root: dirname(logoPath) } });
    expect(r.pages).toBe(1);
  });
});

describe('images in render props', () => {
  // A header image chosen per page, a static image on page 1; the page count is known on pass 2.
  const TwoPages = ({ header, body }: { header: (page: number) => ImageSrc; body?: ImageSrc }) => (
    <Document>
      <Page>
        <Header>
          <View render={({ pageNumber }) => <Image src={header(pageNumber)} className="h-4 w-4" />} />
        </Header>
        {body === undefined ? null : <Image src={body} className="h-4 w-4" />}
        <Text>one</Text>
        <PageBreak />
        <Text>two</Text>
      </Page>
    </Document>
  );
  const files = { localImages: { root: fixtures } };
  const pick = <T,>(page: number, first: T, second: T) => (page === 1 ? first : second);
  const pages = async (
    header: (page: number) => ImageSrc,
    body: ImageSrc | undefined,
    opts: RenderToBufferOptions = {},
  ) =>
    pageImages(await renderToBuffer(<TwoPages header={header} body={body} />, { ...opts, compress: false }));

  it('puts each page its own bytes', async () => {
    expect(await pages((n) => pick(n, logo, palette), photo)).toEqual([[LOGO, PHOTO], [PALETTE]]);
  });

  it('puts each page its own file', async () => {
    const header = (n: number) => pick(n, './logo.png', './palette.png');
    expect(await pages(header, './photo.jpg', files)).toEqual([[LOGO, PHOTO], [PALETTE]]);
  });

  it('puts each page its own URL and request', async () => {
    const opts = allowed();
    const url = (n: number) => `${base}/${pick(n, 'logo.png', 'palette.png')}`;
    expect(await pages(url, `${base}/photo.jpg`, opts)).toEqual([[LOGO, PHOTO], [PALETTE]]);
    const request = (n: number) => ({
      uri: `${base}/image`,
      method: 'POST',
      body: pick(n, 'logo', 'palette'),
    });
    expect(await pages(request, `${base}/photo.jpg`, opts)).toEqual([[LOGO, PHOTO], [PALETTE]]);
  });

  it('calls the functions a rebuild adds, and a static one once', async () => {
    let calls = 0;
    const body = () => {
      calls++;
      return photo;
    };
    expect(await pages((n) => () => pick(n, logo, palette), body)).toEqual([[LOGO, PHOTO], [PALETTE]]);
    expect(calls).toBe(1);
    const header = (n: number) => async () => pick(n, './logo.png', './palette.png');
    expect(await pages(header, undefined, files)).toEqual([[LOGO], [PALETTE]]);
  });

  it('loads them through renderTree and renderSettled too', async () => {
    const template = {
      meta: { id: 'two-pages' },
      schema: z.object({}),
      Template: () => (
        <TwoPages header={(n) => () => pick(n, './logo.png', './palette.png')} body="./photo.jpg" />
      ),
    };
    const opts = { ...files, compress: false };
    const first = renderTree(template, {}, opts);
    const settled = await renderSettled(first, first.images, opts);
    expect(settled.passes).toBe(2);
    expect(pageImages(settled.pdf)).toEqual([[LOGO, PHOTO], [PALETTE]]);
    const r = await generate(template, {}, opts);
    expect(pageImages(r.pdf)).toEqual([[LOGO, PHOTO], [PALETTE]]);
  });
});
