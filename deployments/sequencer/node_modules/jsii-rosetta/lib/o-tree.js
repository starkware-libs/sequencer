"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.OTreeSink = exports.UnknownSyntax = exports.NO_SYNTAX = exports.OTree = void 0;
exports.renderTree = renderTree;
/**
 * "Output" Tree
 *
 * Tree-like structure that holds sequences of trees and strings, which
 * can be rendered to an output sink.
 */
class OTree {
    static simplify(xs) {
        return xs.filter(notUndefined).filter(notEmpty);
    }
    constructor(prefix, children, options = {}) {
        this.options = options;
        this.prefix = OTree.simplify(prefix);
        this.children = OTree.simplify(children ?? []);
        this.attachComment = !!options.canBreakLine;
    }
    /**
     * Set the span in the source file this tree node relates to
     */
    setSpan(start, end) {
        this.span = { start, end };
    }
    write(sink) {
        if (!sink.tagOnce(this.options.renderOnce)) {
            return;
        }
        const meVisible = sink.renderingForSpan(this.span);
        for (const x of this.prefix) {
            sink.write(x);
        }
        const popIndent = sink.requestIndentChange(meVisible ? this.options.indent ?? 0 : 0);
        let mark = sink.mark();
        for (const child of this.children ?? []) {
            if (this.options.separator) {
                if (this.options.trailingSeparator) {
                    sink.ensureNewLine();
                }
                else if (mark.wroteNonWhitespaceSinceMark) {
                    sink.write(this.options.separator);
                }
            }
            mark = sink.mark();
            sink.write(child);
            if (this.options.separator && this.options.trailingSeparator) {
                sink.write(this.options.separator.trimEnd());
            }
        }
        popIndent();
        if (this.options.suffix) {
            if (this.options.separator && this.options.trailingSeparator) {
                sink.ensureNewLine();
            }
            sink.renderingForSpan(this.span);
            sink.write(this.options.suffix);
        }
    }
    get isEmpty() {
        return this.prefix.length + this.children.length === 0;
    }
    toString() {
        return `<INCORRECTLY STRINGIFIED ${this.prefix.toString()}>`;
    }
}
exports.OTree = OTree;
exports.NO_SYNTAX = new OTree([]);
class UnknownSyntax extends OTree {
}
exports.UnknownSyntax = UnknownSyntax;
/**
 * Output sink for OTree objects
 *
 * Maintains state about what has been rendered supports suppressing code
 * fragments based on their tagged source location.
 *
 * Basically: manages the state that was too hard to manage in the
 * tree :).
 */
class OTreeSink {
    constructor(options = {}) {
        this.options = options;
        this.indentLevels = [0];
        this.fragments = new Array();
        this.singletonsRendered = new Set();
        this.pendingIndentChange = 0;
        this.rendering = true;
        this.indentChar = options.indentChar ?? ' ';
    }
    tagOnce(key) {
        if (key === undefined) {
            return true;
        }
        if (this.singletonsRendered.has(key)) {
            return false;
        }
        this.singletonsRendered.add(key);
        return true;
    }
    /**
     * Get a mark for the current sink output location
     *
     * Marks can be used to query about things that have been written to output.
     */
    mark() {
        // eslint-disable-next-line @typescript-eslint/no-this-alias
        const self = this;
        const markIndex = this.fragments.length;
        return {
            get wroteNonWhitespaceSinceMark() {
                return self.fragments.slice(markIndex).some((s) => typeof s !== 'object' && /[^\s]/.exec(s) != null);
            },
        };
    }
    write(text) {
        if (text instanceof OTree) {
            text.write(this);
        }
        else {
            if (!this.rendering) {
                return;
            }
            if (containsNewline(text)) {
                this.applyPendingIndentChange();
            }
            this.append(text.replace(/\n/g, `\n${this.indentChar.repeat(this.currentIndent)}`));
        }
    }
    /**
     * Ensures the following tokens will be output on a new line (emits a new line
     * and indent unless immediately preceded or followed by a newline, ignoring
     * surrounding white space).
     */
    ensureNewLine() {
        this.applyPendingIndentChange();
        this.fragments.push({ conditionalNewLine: { indent: this.currentIndent } });
    }
    renderingForSpan(span) {
        if (span && this.options.visibleSpans) {
            this.rendering = this.options.visibleSpans.fullyContainsSpan(span);
        }
        return this.rendering;
    }
    requestIndentChange(x) {
        if (x === 0) {
            return () => undefined;
        }
        this.pendingIndentChange = x;
        const currentIndentState = this.indentLevels.length;
        // Return a pop function which will reset to the current indent state,
        // regardless of whether the indent was actually applied or not.
        return () => {
            this.indentLevels.splice(currentIndentState);
            this.pendingIndentChange = 0;
        };
    }
    toString() {
        // Strip trailing whitespace from every line, and empty lines from the start and end
        return this.fragments
            .map((item, index, fragments) => {
            if (typeof item !== 'object') {
                return item;
            }
            const ignore = '';
            const leading = fragments.slice(0, index).reverse();
            for (const fragment of leading) {
                if (typeof fragment === 'object') {
                    // We don't emit if there was already a conditional newline just before
                    return ignore;
                }
                // If there's a trailing newline, then we don't emit this one
                if (/\n\s*$/m.exec(fragment)) {
                    return ignore;
                }
                // If it contained non-whitespace characters, we need to check trailing data...
                if (/[^\s]/.exec(fragment)) {
                    break;
                }
            }
            const newlineAndIndent = `\n${this.indentChar.repeat(item.conditionalNewLine.indent)}`;
            const trailing = fragments.slice(index + 1);
            for (const fragment of trailing) {
                if (typeof fragment === 'object') {
                    // We're the first of a sequence, so we must emit (unless we returned earlier, of course)
                    return newlineAndIndent;
                }
                // If there's a leading newline, then we don't emit this one
                if (/^\s*\n/m.exec(fragment)) {
                    return ignore;
                }
                // If it contained non-whitespace characters, we emit this one
                if (/[^\s]/.exec(fragment)) {
                    return newlineAndIndent;
                }
            }
            return ignore;
        })
            .join('')
            .replace(/[ \t]+$/gm, '')
            .replace(/^\n+/, '')
            .replace(/\n+$/, '');
    }
    append(x) {
        this.fragments.push(x);
    }
    applyPendingIndentChange() {
        if (this.pendingIndentChange !== 0) {
            this.indentLevels.push(this.currentIndent + this.pendingIndentChange);
            this.pendingIndentChange = 0;
        }
    }
    get currentIndent() {
        return this.indentLevels[this.indentLevels.length - 1];
    }
}
exports.OTreeSink = OTreeSink;
function notUndefined(x) {
    return x !== undefined;
}
function notEmpty(x) {
    return x instanceof OTree ? !x.isEmpty : x !== '';
}
function renderTree(tree, options) {
    const sink = new OTreeSink(options);
    tree.write(sink);
    return sink.toString();
}
function containsNewline(x) {
    return x.includes('\n');
}
//# sourceMappingURL=o-tree.js.map