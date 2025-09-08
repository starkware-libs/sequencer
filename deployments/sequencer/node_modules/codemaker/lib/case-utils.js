"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.toCamelCase = toCamelCase;
exports.toPascalCase = toPascalCase;
exports.toSnakeCase = toSnakeCase;
const camelcase = require("camelcase");
// eslint-disable-next-line @typescript-eslint/no-require-imports
const decamelize = require("decamelize");
const COMMON_ABBREVIATIONS = ['KiB', 'MiB', 'GiB'];
function toCamelCase(...args) {
    return camelcase(args);
}
function toPascalCase(...args) {
    return camelcase(args, { pascalCase: true });
}
const ABBREV_RE = new RegExp(`(^|[^A-Z])(${COMMON_ABBREVIATIONS.map(regexQuote).join('|')})($|[^a-z])`, 'g');
function toSnakeCase(s, separator = '_') {
    // Save common abbrevations
    s = s.replace(ABBREV_RE, (_, before, abbr, after) => before + ucfirst(abbr.toLowerCase()) + after);
    return decamelize(s, { separator });
    function ucfirst(str) {
        return str.slice(0, 1).toUpperCase() + str.slice(1).toLowerCase();
    }
}
function regexQuote(s) {
    return s.replace(/[.?*+^$[\]\\(){}|-]/g, '\\$&');
}
//# sourceMappingURL=case-utils.js.map