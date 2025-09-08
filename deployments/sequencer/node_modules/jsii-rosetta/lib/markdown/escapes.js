"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.makeXmlEscaper = makeXmlEscaper;
exports.makeJavaEscaper = makeJavaEscaper;
/**
 * Make a generic XML escaper
 */
function makeXmlEscaper() {
    const attr = [...TEXT, ...ATTR_ADDL];
    return {
        text: (x) => escapeText(TEXT, x),
        attribute: (x) => escapeText(attr, x),
        text2attr: (x) => escapeText(ATTR_ADDL, x),
    };
}
/**
 * Make a Java specific escaper
 *
 * This one also escapes '@' because that triggers parsing of comment directives
 * in Java.
 */
function makeJavaEscaper() {
    const javaText = [...TEXT, [new RegExp('@', 'g'), '&#64;']];
    const javaAttr = [...javaText, ...ATTR_ADDL];
    return {
        text: (x) => escapeText(javaText, x),
        attribute: (x) => escapeText(javaAttr, x),
        text2attr: (x) => escapeText(ATTR_ADDL, x),
    };
}
const TEXT = [
    [new RegExp('&', 'g'), '&amp;'],
    [new RegExp('<', 'g'), '&lt;'],
    [new RegExp('>', 'g'), '&gt;'],
];
// Additional escapes (in addition to the text escapes) which need to be escaped inside attributes.
const ATTR_ADDL = [
    [new RegExp('"', 'g'), '&quot;'],
    [new RegExp("'", 'g'), '&apos;'],
];
function escapeText(set, what) {
    if (!what) {
        return '';
    }
    for (const [re, repl] of set) {
        what = what.replace(re, repl);
    }
    return what;
}
//# sourceMappingURL=escapes.js.map