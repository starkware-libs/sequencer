"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.TargetLanguage = void 0;
exports.targetName = targetName;
exports.supportsTransitiveSubmoduleAccess = supportsTransitiveSubmoduleAccess;
const assert = require("node:assert");
var TargetLanguage;
(function (TargetLanguage) {
    TargetLanguage["PYTHON"] = "python";
    TargetLanguage["CSHARP"] = "csharp";
    TargetLanguage["JAVA"] = "java";
    TargetLanguage["GO"] = "go";
    /** @internal an alias of PYTHON to make intent clear when language is irrelevant, must be last */
    TargetLanguage["VISUALIZE"] = "python";
})(TargetLanguage || (exports.TargetLanguage = TargetLanguage = {}));
const VALID_TARGET_LANGUAGES = new Set(Object.values(TargetLanguage));
function targetName(language) {
    // The TypeScript compiler should guarantee the below `switch` statement covers all possible
    // values of the TargetLanguage enum, but we add an assert here for clarity of intent.
    assert(VALID_TARGET_LANGUAGES.has(language), `Invalid/unexpected target language identifier: ${language}`);
    switch (language) {
        case TargetLanguage.VISUALIZE:
        case TargetLanguage.PYTHON:
            return 'python';
        case TargetLanguage.CSHARP:
            return 'dotnet';
        case TargetLanguage.JAVA:
            return 'java';
        case TargetLanguage.GO:
            return 'go';
    }
}
/**
 * Determines whether the supplied language supports transitive submodule
 * access (similar to how TypeScript/Javascript allows to use a partially
 * qualified name to access a namespace-nested value).
 *
 * If `true`, imports will mirror those found in the original TypeScript
 * code, namespace-traversing property accesses will be rendered as such. This
 * means the following snippet would be transformed "as-is":
 * ```ts
 * import * as cdk from 'aws-cdk-lib';
 * new cdk.aws_s3.Bucket(this, 'Bucket');
 * ```
 *
 * If `false` on the other hand, each used submodule will be imported
 * separately and namespace-traversing property accesses will be replaced with
 * references to the separately-imported submodule. This means the above
 * snippet would be transformed as if it had been modifired to:
 * ```ts
 * import * as aws_s3 from 'aws-cdk-lib/aws-s3';
 * new aws_s3.Bucket(this, 'Bucket');
 * ```
 */
function supportsTransitiveSubmoduleAccess(language) {
    switch (language) {
        case TargetLanguage.VISUALIZE:
        case TargetLanguage.PYTHON:
            return true;
        case TargetLanguage.CSHARP:
            return true;
        case TargetLanguage.JAVA:
            return false;
        case TargetLanguage.GO:
            return false;
    }
}
//# sourceMappingURL=target-language.js.map