"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.die = die;
exports.toPythonIdentifier = toPythonIdentifier;
const PYTHON_KEYWORDS = new Set([
    'False',
    'None',
    'True',
    'and',
    'as',
    'assert',
    'async',
    'await',
    'break',
    'class',
    'continue',
    'def',
    'del',
    'elif',
    'else',
    'except',
    'finally',
    'for',
    'from',
    'global',
    'if',
    'import',
    'in',
    'is',
    'lambda',
    'nonlocal',
    'not',
    'or',
    'pass',
    'raise',
    'return',
    'try',
    'while',
    'with',
    'yield',
]);
function die(message) {
    throw new Error(message);
}
function toPythonIdentifier(name) {
    if (PYTHON_KEYWORDS.has(name)) {
        return `${name}_`;
    }
    return name;
}
//# sourceMappingURL=util.js.map