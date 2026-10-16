// A short letter in German, hyphenated, with a character Inter lacks: the page fetches the
// engine's German hyphenation patterns and Noto Sans, the fallback font, only for it. Rendered in
// Node too, by the tests, for the same PDF.
import { Document, Page, Text } from '@inkoxide/render';

export default function Letter() {
  return (
    <Document title="Brief" lang="de">
      <Page size="A6" style={{ padding: 24, fontSize: 10 }}>
        <Text style={{ fontSize: 14, fontWeight: 'bold' }}>Sehr geehrte Damen und Herren,</Text>
        <Text hyphenate style={{ width: 120, textAlign: 'justify', marginTop: 8 }}>
          die Donaudampfschifffahrtsgesellschaft bestätigt Ihre Reservierung. Die Kapitänskajüte ist
          vorbereitet, und das Frühstücksbuffet erwartet Sie.
        </Text>
        <Text style={{ marginTop: 8 }}>{'Mit freundlichen Grüßen, Ǆukić'}</Text>
      </Page>
    </Document>
  );
}
