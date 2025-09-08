"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.typeSystemFromSource = typeSystemFromSource;
exports.assemblyFromSource = assemblyFromSource;
const jsii_1 = require("jsii");
const lib_1 = require("../lib");
function typeSystemFromSource(source, cb) {
    const asm = assemblyFromSource(source, cb);
    return asm.system;
}
function assemblyFromSource(source, cb) {
    const ass = (0, jsii_1.sourceToAssemblyHelper)(source, cb);
    const ts = new lib_1.TypeSystem();
    return ts.addAssembly(new lib_1.Assembly(ts, ass));
}
//# sourceMappingURL=util.js.map