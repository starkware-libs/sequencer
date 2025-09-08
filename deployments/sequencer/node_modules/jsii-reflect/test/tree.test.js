"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const path_1 = require("path");
const tree_1 = require("../lib/tree");
const type_system_1 = require("../lib/type-system");
const typeSystem = new type_system_1.TypeSystem();
beforeAll(() => typeSystem.loadModule((0, path_1.dirname)(require.resolve('jsii-calc/package.json'))));
test('defaults', () => {
    const stream = new StringWriter();
    new tree_1.TypeSystemTree(typeSystem, { colors: false }).printTree(stream);
    expect(stream.stringContent).toMatchSnapshot();
});
test('showAll', () => {
    const stream = new StringWriter();
    new tree_1.TypeSystemTree(typeSystem, { colors: false, showAll: true }).printTree(stream);
    expect(stream.stringContent).toMatchSnapshot();
});
test('types', () => {
    const stream = new StringWriter();
    new tree_1.TypeSystemTree(typeSystem, { colors: false, types: true }).printTree(stream);
    expect(stream.stringContent).toMatchSnapshot();
});
test('members', () => {
    const stream = new StringWriter();
    new tree_1.TypeSystemTree(typeSystem, { colors: false, members: true }).printTree(stream);
    expect(stream.stringContent).toMatchSnapshot();
});
test('inheritance', () => {
    const stream = new StringWriter();
    new tree_1.TypeSystemTree(typeSystem, {
        colors: false,
        inheritance: true,
    }).printTree(stream);
    expect(stream.stringContent).toMatchSnapshot();
});
test('signatures', () => {
    const stream = new StringWriter();
    new tree_1.TypeSystemTree(typeSystem, { colors: false, signatures: true }).printTree(stream);
    expect(stream.stringContent).toMatchSnapshot();
});
class StringWriter {
    constructor() {
        this.buffer = Buffer.alloc(0);
    }
    write(chunk, _encoding, cb) {
        if (Buffer.isBuffer(chunk)) {
            this.buffer = Buffer.concat([this.buffer, chunk]);
        }
        else {
            this.buffer = Buffer.concat([this.buffer, Buffer.from(chunk)]);
        }
        if (cb != null) {
            cb();
        }
        return true;
    }
    get stringContent() {
        return this.buffer.toString('utf-8');
    }
}
//# sourceMappingURL=tree.test.js.map