"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.TARGET_LANGUAGES = exports.TargetLanguage = void 0;
exports.getVisitorFromLanguage = getVisitorFromLanguage;
const csharp_1 = require("./csharp");
const go_1 = require("./go");
const java_1 = require("./java");
const python_1 = require("./python");
const target_language_1 = require("./target-language");
Object.defineProperty(exports, "TargetLanguage", { enumerable: true, get: function () { return target_language_1.TargetLanguage; } });
const visualize_1 = require("./visualize");
exports.TARGET_LANGUAGES = {
    [target_language_1.TargetLanguage.PYTHON]: {
        version: python_1.PythonVisitor.VERSION,
        createVisitor: () => new python_1.PythonVisitor(),
    },
    [target_language_1.TargetLanguage.CSHARP]: {
        version: csharp_1.CSharpVisitor.VERSION,
        createVisitor: () => new csharp_1.CSharpVisitor(),
    },
    [target_language_1.TargetLanguage.JAVA]: {
        version: java_1.JavaVisitor.VERSION,
        createVisitor: () => new java_1.JavaVisitor(),
    },
    [target_language_1.TargetLanguage.GO]: {
        version: go_1.GoVisitor.VERSION,
        createVisitor: () => new go_1.GoVisitor(),
    },
};
function getVisitorFromLanguage(language) {
    if (language !== undefined) {
        const target = Object.values(target_language_1.TargetLanguage).find((t) => t === language);
        if (target === undefined) {
            throw new Error(`Unknown target language: ${language}. Expected one of ${Object.values(target_language_1.TargetLanguage).join(', ')}`);
        }
        return exports.TARGET_LANGUAGES[target].createVisitor();
    }
    // Default to visualizing AST, including nodes we don't recognize yet
    return new visualize_1.VisualizeAstVisitor();
}
//# sourceMappingURL=index.js.map