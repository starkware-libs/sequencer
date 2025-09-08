"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const validator_1 = require("../validator");
// A rule set defining all compilerOptions that are configurable via the jsii field in package.json
// This is an internal rule set, that may be used by other rule sets.
// We accept all value for these
const jsiiConfiguredOptions = new validator_1.RuleSet();
jsiiConfiguredOptions.shouldPass('outDir', validator_1.Match.ANY);
jsiiConfiguredOptions.shouldPass('rootDir', validator_1.Match.ANY);
jsiiConfiguredOptions.shouldPass('forceConsistentCasingInFileNames', validator_1.Match.ANY);
jsiiConfiguredOptions.shouldPass('declarationMap', validator_1.Match.ANY);
jsiiConfiguredOptions.shouldPass('inlineSourceMap', validator_1.Match.ANY);
jsiiConfiguredOptions.shouldPass('inlineSources', validator_1.Match.ANY);
jsiiConfiguredOptions.shouldPass('sourceMap', validator_1.Match.ANY);
jsiiConfiguredOptions.shouldPass('types', validator_1.Match.ANY);
jsiiConfiguredOptions.shouldPass('baseUrl', validator_1.Match.ANY);
jsiiConfiguredOptions.shouldPass('paths', validator_1.Match.ANY);
jsiiConfiguredOptions.shouldPass('composite', validator_1.Match.ANY); // configured via projectReferences
jsiiConfiguredOptions.shouldPass('tsBuildInfoFile', validator_1.Match.ANY);
exports.default = jsiiConfiguredOptions;
//# sourceMappingURL=jsii-configured-options.js.map