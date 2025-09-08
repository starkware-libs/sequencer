"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.parse = parse;
exports.stringify = stringify;
const node_stream_1 = require("node:stream");
const node_util_1 = require("node:util");
const stream_json_1 = require("stream-json");
const Assembler = require("stream-json/Assembler");
const Disassembler_1 = require("stream-json/Disassembler");
const Stringer_1 = require("stream-json/Stringer");
// NB: In node 15+, there is a node:stream.promises object that has this built-in.
const asyncPipeline = (0, node_util_1.promisify)(node_stream_1.pipeline);
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
function parse(reader) {
    const assembler = new Assembler();
    const jsonParser = (0, stream_json_1.parser)();
    assembler.connectTo(jsonParser);
    return asyncPipeline(reader, jsonParser).then(() => assembler.current);
}
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
async function stringify(value, ...writers) {
    const reader = new node_stream_1.Readable({ objectMode: true });
    reader.push(value);
    reader.push(null);
    return asyncPipeline(reader, (0, Disassembler_1.disassembler)(), (0, Stringer_1.stringer)(), ...writers);
}
//# sourceMappingURL=json.js.map