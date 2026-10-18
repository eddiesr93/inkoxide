import type { XmpSchema } from '@inkoxide/schema';

/** The Factur-X (and ZUGFeRD 2) profiles, as `ConformanceLevel` names them. */
export type FacturXProfile = 'MINIMUM' | 'BASIC WL' | 'BASIC' | 'EN 16931' | 'EXTENDED' | 'XRECHNUNG';

export interface FacturXOptions {
  /** The profile of the XML invoice. */
  profile: FacturXProfile;
  /** The attachment's name. Default `'factur-x.xml'`. */
  fileName?: string;
  /** Default `'INVOICE'`. */
  documentType?: 'INVOICE' | 'ORDER' | 'ORDER_RESPONSE' | 'ORDER_CHANGE';
  /** The version of the Factur-X XML schema. Default `'1.0'`. */
  version?: string;
}

/**
 * The metadata of a Factur-X (or ZUGFeRD 2) e-invoice, for `<Document xmpExtensions>`: the `fx`
 * schema with the invoice's file name, type, version and profile. The document also needs
 * `conformance="PDF/A-3b"` (or `"PDF/A-3a"`) and the XML in `attachments`, named `fileName`, with
 * `mimeType: 'text/xml'` and the relationship of `facturXRelationship(profile)`.
 */
export function facturX(opts: FacturXOptions): XmpSchema {
  return {
    schema: 'Factur-X PDFA Extension Schema',
    namespace: 'urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#',
    prefix: 'fx',
    properties: [
      {
        name: 'DocumentFileName',
        value: opts.fileName ?? 'factur-x.xml',
        description: 'The name of the embedded XML document',
      },
      {
        name: 'DocumentType',
        value: opts.documentType ?? 'INVOICE',
        description: 'The type of the hybrid document in capital letters, e.g. INVOICE or ORDER',
      },
      {
        name: 'Version',
        value: opts.version ?? '1.0',
        description: 'The actual version of the standard applying to the embedded XML document',
      },
      {
        name: 'ConformanceLevel',
        value: opts.profile,
        description: 'The conformance level of the embedded XML document',
      },
    ],
  };
}

/**
 * The relationship of the XML of a Factur-X invoice of `profile` to the PDF: `'data'` for the
 * profiles that are not a full invoice (MINIMUM, BASIC WL), whose PDF is the invoice, and
 * `'alternative'` for the others, whose XML is the invoice in another form.
 */
export function facturXRelationship(profile: FacturXProfile): 'data' | 'alternative' {
  return profile === 'MINIMUM' || profile === 'BASIC WL' ? 'data' : 'alternative';
}
