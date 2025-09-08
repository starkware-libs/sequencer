"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __exportStar = (this && this.__exportStar) || function(m, exports) {
    for (var p in m) if (p !== "default" && !Object.prototype.hasOwnProperty.call(exports, p)) __createBinding(exports, m, p);
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.GoVisitor = exports.PythonVisitor = exports.JavaVisitor = exports.CSharpVisitor = exports.TargetLanguage = exports.renderTree = void 0;
__exportStar(require("./translate"), exports);
var o_tree_1 = require("./o-tree");
Object.defineProperty(exports, "renderTree", { enumerable: true, get: function () { return o_tree_1.renderTree; } });
var target_language_1 = require("./languages/target-language");
Object.defineProperty(exports, "TargetLanguage", { enumerable: true, get: function () { return target_language_1.TargetLanguage; } });
var csharp_1 = require("./languages/csharp");
Object.defineProperty(exports, "CSharpVisitor", { enumerable: true, get: function () { return csharp_1.CSharpVisitor; } });
var java_1 = require("./languages/java");
Object.defineProperty(exports, "JavaVisitor", { enumerable: true, get: function () { return java_1.JavaVisitor; } });
var python_1 = require("./languages/python");
Object.defineProperty(exports, "PythonVisitor", { enumerable: true, get: function () { return python_1.PythonVisitor; } });
var go_1 = require("./languages/go");
Object.defineProperty(exports, "GoVisitor", { enumerable: true, get: function () { return go_1.GoVisitor; } });
__exportStar(require("./tablets/tablets"), exports);
__exportStar(require("./rosetta-reader"), exports);
__exportStar(require("./rosetta-translator"), exports);
__exportStar(require("./snippet"), exports);
__exportStar(require("./markdown"), exports);
__exportStar(require("./commands/transliterate"), exports);
__exportStar(require("./strict"), exports);
//# sourceMappingURL=index.js.map