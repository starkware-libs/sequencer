"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const jsii_configured_options_1 = require("./jsii-configured-options");
const validator_1 = require("../validator");
// A rule set defining all compilerOptions that can be configured by users with or without constraints.
// These are options jsii doesn't have a particular opinion about
// This is an internal rule set, that may be used by other rule sets.
const configurableOptions = new validator_1.RuleSet();
// import all options that are configurable via jsii settings
configurableOptions.import(jsii_configured_options_1.default);
// options jsii allows to be configured
configurableOptions.shouldPass('incremental', validator_1.Match.ANY);
configurableOptions.shouldPass('noImplicitReturns', validator_1.Match.ANY);
configurableOptions.shouldPass('noUnusedLocals', validator_1.Match.ANY);
configurableOptions.shouldPass('noUnusedParameters', validator_1.Match.ANY);
configurableOptions.shouldPass('resolveJsonModule', validator_1.Match.ANY);
configurableOptions.shouldPass('experimentalDecorators', validator_1.Match.ANY);
configurableOptions.shouldPass('noFallthroughCasesInSwitch', validator_1.Match.ANY);
configurableOptions.shouldPass('verbatimModuleSyntax', validator_1.Match.ANY);
configurableOptions.shouldPass('isolatedModules', validator_1.Match.ANY);
configurableOptions.shouldPass('isolatedDeclarations', validator_1.Match.ANY);
exports.default = configurableOptions;
//# sourceMappingURL=configurable-options.js.map