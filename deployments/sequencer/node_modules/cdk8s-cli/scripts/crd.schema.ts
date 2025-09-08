/***************************************************************************************************
 * Script to generate the JSONSchema of a CRD.
 *
 * Based on the 'ManifestObjectDefinition' TypeScript definition we use in src/import/crd.ts
 * It is used to validate CRD manifests when running 'cdk8s import <crd-url>'.
 *
 */
import * as tjs from 'typescript-json-schema';
import * as path from 'path';
import * as fs from 'fs';

const out = path.join(__dirname, '..', 'src', 'schemas', `crd.schema.json`);

const settings: Partial<tjs.Args> = {

  // we want to create the "required" array to validate
  // required properties
  required: true,

  // we must allow extra props because our code defined schema
  // only defines keys that we actually use, which is ok.
  noExtraProps: false,

  // define the output file.
  // this is a bit strange since we have to write this
  // file ourselves, but from some reason this property is required.
  out: out,
};

const program = tjs.getProgramFromFiles([path.join(__dirname, '../lib/import/crd.d.ts')]);
const schema = tjs.generateSchema(program, 'ManifestObjectDefinition', settings);
fs.writeFileSync(out, JSON.stringify(schema, null, 4));