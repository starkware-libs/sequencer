import { Readable } from 'node:stream';
/**
 * Asynchronously parses a single JSON value from the provided reader. The JSON
 * text might be longer than what could fit in a single string value, since the
 * processing is done in a streaming manner.
 *
 * Prefer using JSON.parse if you know the entire JSON text is always small
 * enough to fit in a string value, as this would have better performance.
 *
 * @param reader the reader from which to consume JSON text.
 *
 * @returns the parse JSON value as a Javascript value.
 */
export declare function parse(reader: Readable): Promise<any>;
/**
 * Serializes a possibly large object into the provided writer. The object may
 * be large enough that the JSON text cannot fit in a single string value.
 *
 * Prefer using JSON.stringify if you know the object is always small enough
 * that the JSON text can fit in a single string value, as this would have
 * better performance.
 *
 * @param value the value to be serialized.
 * @param writers the sequence of write streams to use to output the JSON text.
 */
export declare function stringify(value: any, ...writers: Array<NodeJS.ReadWriteStream | NodeJS.WritableStream>): Promise<void>;
//# sourceMappingURL=json.d.ts.map