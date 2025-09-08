"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const validator_1 = require("../validator");
// A rule set defining all compilerOptions that are explicitly known to be incompatible with jsii
// This is an internal rule set, that may be used by other rule sets.
const incompatibleOptions = new validator_1.RuleSet();
incompatibleOptions.shouldFail('noEmit', validator_1.Match.TRUE);
incompatibleOptions.shouldFail('noLib', validator_1.Match.TRUE);
incompatibleOptions.shouldFail('declaration', validator_1.Match.FALSE);
incompatibleOptions.shouldFail('emitDeclarationOnly', validator_1.Match.TRUE);
exports.default = incompatibleOptions;
//# sourceMappingURL=incompatible-options.js.map