"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const incompatible_options_1 = require("./incompatible-options");
const validator_1 = require("../validator");
// The public rule set used for the "minimal" tsconfig validation setting
// To goal of this rule set is to only prevent obvious misconfigurations,
// while leaving everything else up to the user.
const minimal = new validator_1.RuleSet();
minimal.import(incompatible_options_1.default);
exports.default = minimal;
//# sourceMappingURL=minimal.public.js.map