"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.translateMarkdown = translateMarkdown;
const markdown_1 = require("../markdown/markdown");
const markdown_renderer_1 = require("../markdown/markdown-renderer");
const replace_typescript_transform_1 = require("../markdown/replace-typescript-transform");
const translate_1 = require("../translate");
function translateMarkdown(markdown, visitor, opts = {}) {
    const translator = new translate_1.Translator(false);
    const location = { api: 'file', fileName: markdown.fileName };
    const translatedMarkdown = (0, markdown_1.transformMarkdown)(markdown.contents, new markdown_renderer_1.MarkdownRenderer(), new replace_typescript_transform_1.ReplaceTypeScriptTransform(location, opts.strict ?? false, (tsSnippet) => {
        const translated = translator.translatorFor(tsSnippet).renderUsing(visitor);
        return {
            language: opts.languageIdentifier ?? '',
            source: translated,
        };
    }));
    return {
        translation: translatedMarkdown,
        diagnostics: translator.diagnostics,
    };
}
//# sourceMappingURL=convert.js.map