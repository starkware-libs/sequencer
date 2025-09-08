"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.INCOMPLETE_DISCLAIMER_NONCOMPILING = exports.ALL_BUILDERS = exports.TargetName = void 0;
const builder_1 = require("../builder");
const util_1 = require("../util");
const dotnet_1 = require("./dotnet");
const go_1 = require("./go");
const java_1 = require("./java");
const js_1 = require("./js");
const python_1 = require("./python");
var TargetName;
(function (TargetName) {
    TargetName["DOTNET"] = "dotnet";
    TargetName["GO"] = "go";
    TargetName["JAVA"] = "java";
    TargetName["JAVASCRIPT"] = "js";
    TargetName["PYTHON"] = "python";
})(TargetName || (exports.TargetName = TargetName = {}));
exports.ALL_BUILDERS = {
    dotnet: (ms, o) => new dotnet_1.DotnetBuilder((0, util_1.flatten)(ms), o),
    go: (ms, o) => new builder_1.IndependentPackageBuilder(TargetName.GO, go_1.Golang, ms, o),
    java: (ms, o) => new java_1.JavaBuilder((0, util_1.flatten)(ms), o),
    js: (ms, o) => new builder_1.IndependentPackageBuilder(TargetName.JAVASCRIPT, js_1.default, ms, o),
    python: (ms, o) => new builder_1.IndependentPackageBuilder(TargetName.PYTHON, python_1.default, ms, o),
};
exports.INCOMPLETE_DISCLAIMER_NONCOMPILING = 'Example automatically generated from non-compiling source. May contain errors.';
//# sourceMappingURL=index.js.map