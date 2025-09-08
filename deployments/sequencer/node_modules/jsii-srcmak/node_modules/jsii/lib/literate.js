"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.typescriptSourceToMarkdown = typescriptSourceToMarkdown;
exports.includeAndRenderExamples = includeAndRenderExamples;
exports.loadFromFile = loadFromFile;
exports.contentToLines = contentToLines;
exports.fileSystemLoader = fileSystemLoader;
/**
 * A tiny module to include annotated (working!) code snippets into the documentation
 *
 * Not using 'literate-programming' or 'erasumus' projects because they work
 * the other way around: take code from MarkDown, save it as a file, then
 * execute that.
 *
 * We do the opposite: start from source code annotated with MarkDown and
 * extract it into (larger) MarkDown files.
 *
 * Including into README
 * ---------------------
 *
 * To include the examples directly into the README, make a link to the
 * annotated TypeScript file on a line by itself, and make sure the
 * extension of the file ends in `.lit.ts`.
 *
 * For example:
 *
 *    [example](test/integ.bucket.lit.ts)
 *
 * Annotating source
 * -----------------
 *
 * We use the triple-slash comment for our directives, since it's valid TypeScript
 * and are treated as regular comments if not the very first thing in the file.
 *
 * By default, the whole file is included, unless the source contains the statement
 * "/// !show". For example:
 *
 *     a
 *     /// !show
 *     b
 *     /// !hide
 *     c
 *
 * In this example, only 'b' would be included in the output. A single file may
 * switching including and excluding on and off multiple times in the same file.
 *
 * Other lines starting with triple slashes will be rendered as Markdown in between
 * the source lines. For example:
 *
 *     const x = 1;
 *     /// Now we're going to print x:
 *     console.log(x);
 *
 * Will be rendered as:
 *
 *     ```ts
 *     const x = 1;
 *     ```
 *
 *     Now we're going to print x:
 *
 *     ```ts
 *     console.log(x);
 *     ```
 */
const fs = require("node:fs");
const path = require("node:path");
/**
 * Convert an annotated TypeScript source file to MarkDown
 */
function typescriptSourceToMarkdown(lines, codeBlockAnnotations) {
    const relevantLines = findRelevantLines(lines);
    const markdownLines = markdownify(relevantLines, codeBlockAnnotations);
    return markdownLines;
}
/**
 * Given MarkDown source, find source files to include and render
 *
 * We recognize links on a line by themselves if the link text starts
 * with the string "example" (case insensitive). For example:
 *
 *     [example](test/integ.bucket.ts)
 */
function includeAndRenderExamples(lines, loader, projectRoot) {
    const ret = [];
    const regex = /^\[([^\]]*)\]\(([^)]+\.lit\.ts)\)/i;
    for (const line of lines) {
        const m = regex.exec(line);
        if (m) {
            // Found an include
            const filename = m[2];
            // eslint-disable-next-line no-await-in-loop
            const { lines: source, fullPath } = loader(filename);
            // 'lit' source attribute will make snippet compiler know to extract the same source
            // Needs to be relative to the project root.
            const imported = typescriptSourceToMarkdown(source, [`lit=${toUnixPath(path.relative(projectRoot, fullPath))}`]);
            ret.push(...imported);
        }
        else {
            ret.push(line);
        }
    }
    return ret;
}
/**
 * Load a file into a string array
 */
function loadFromFile(fileName) {
    const content = fs.readFileSync(fileName, { encoding: 'utf-8' });
    return contentToLines(content);
}
/**
 * Turn file content string into an array of lines ready for processing using the other functions
 */
function contentToLines(content) {
    return content.split('\n').map((x) => x.trimRight());
}
/**
 * Return a file system loader given a base directory
 */
function fileSystemLoader(directory) {
    return (fileName) => {
        const fullPath = path.resolve(directory, fileName);
        return { fullPath, lines: loadFromFile(fullPath) };
    };
}
const RELEVANT_TAG = '/// !show';
const DETAIL_TAG = '/// !hide';
const INLINE_MD_REGEX = /^\s*\/\/\/ (.*)$/;
/**
 * Find the relevant lines of the input source
 *
 * Respects switching tags, returns everything if no switching found.
 *
 * Strips common indentation from the blocks it finds.
 */
function findRelevantLines(lines) {
    let inRelevant = false;
    let didFindRelevant = false;
    const ret = [];
    for (const line of lines) {
        if (line.trim() === RELEVANT_TAG) {
            inRelevant = true;
            didFindRelevant = true;
        }
        else if (line.trim() === DETAIL_TAG) {
            inRelevant = false;
        }
        else {
            if (inRelevant) {
                ret.push(line);
            }
        }
    }
    // Return full lines list if no switching found
    return stripCommonIndent(didFindRelevant ? ret : lines);
}
/**
 * Remove common leading whitespace from the given lines
 */
function stripCommonIndent(lines) {
    const leadingWhitespace = /^(\s*)/;
    const indents = lines.map((l) => leadingWhitespace.exec(l)[1].length);
    const commonIndent = Math.min(...indents);
    return lines.map((l) => l.slice(commonIndent));
}
/**
 * Turn source lines into Markdown, starting in TypeScript mode
 */
function markdownify(lines, codeBlockAnnotations) {
    const typescriptLines = [];
    const ret = [];
    for (const line of lines) {
        const m = INLINE_MD_REGEX.exec(line);
        if (m) {
            // Literal MarkDown line
            flushTS();
            ret.push(m[1]);
        }
        else {
            typescriptLines.push(line);
        }
    }
    flushTS();
    return ret;
    /**
     * Flush typescript lines with a triple-backtick-ts block around it.
     */
    function flushTS() {
        if (typescriptLines.length !== 0) {
            // eslint-disable-next-line prefer-template
            ret.push(`\`\`\`ts${codeBlockAnnotations.length > 0 ? ` ${codeBlockAnnotations.join(' ')}` : ''}`, ...typescriptLines, '```');
            typescriptLines.splice(0); // Clear
        }
    }
}
function toUnixPath(x) {
    return x.replace(/\\/g, '/');
}
//# sourceMappingURL=literate.js.map