"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Spans = void 0;
exports.trimCompleteSourceToVisible = trimCompleteSourceToVisible;
exports.spanInside = spanInside;
exports.spanContains = spanContains;
/**
 * A class representing a set of non-overlapping Spans.
 */
class Spans {
    /**
     * Derive visible spans from marked source (`/// !show` and `/// !hide` directives).
     */
    static visibleSpansFromSource(source) {
        return new Spans(calculateMarkedSpans(source).filter((s) => s.visible));
    }
    constructor(_spans) {
        this._spans = _spans;
        _spans.sort((a, b) => a.start - b.start);
        // Merge adjacent spans
        let i = 0;
        while (i < this._spans.length - 1) {
            const current = this._spans[i];
            const next = this._spans[i + 1];
            if (current.end === next.start) {
                // Replace these two with a new, merged one
                this._spans.splice(i, 2, {
                    start: current.start,
                    end: next.end,
                });
            }
            else {
                // Else advance
                i++;
            }
        }
    }
    get spans() {
        return this._spans;
    }
    /**
     * Whether another span is fully contained within this set of spans
     */
    fullyContainsSpan(span) {
        const candidate = this.findSpan(span.start);
        return !!candidate && spanInside(span, candidate);
    }
    containsPosition(pos) {
        const candidate = this.findSpan(pos);
        return !!candidate && spanContains(candidate, pos);
    }
    /**
     * Return whether the START of the given node is visible
     *
     * For nodes that potentially span many lines (like class declarations)
     * this will check the first line.
     */
    containsStartOfNode(node) {
        return this.containsPosition(node.getStart());
    }
    /**
     * Find the span that would contain the given position, if any
     *
     * Returns the highest span s.t. span.start <= position. Uses the fact that
     * spans are non-overlapping.
     */
    findSpan(position) {
        // For now, using linear search as the amount of spans is rather trivial.
        // Change to binary search if this ever becomes an issue
        if (this.spans.length === 0 || position < this._spans[0].start) {
            return undefined;
        }
        let candidate = this._spans[0];
        let i = 1;
        while (i < this.spans.length && this.spans[i].start <= position) {
            candidate = this._spans[i];
            i++;
        }
        return candidate;
    }
}
exports.Spans = Spans;
function trimCompleteSourceToVisible(source) {
    const spans = Spans.visibleSpansFromSource(source);
    return spans.spans
        .map((span) => source.substring(span.start, span.end))
        .join('')
        .trimRight();
}
function calculateMarkedSpans(source) {
    const regEx = /^[ \t]*[/]{3}[ \t]*(!(?:show|hide))[ \t]*$/gm;
    const ret = new Array();
    let match;
    let spanStart;
    let visible = true;
    while ((match = regEx.exec(source)) != null) {
        const directiveStart = match.index;
        const directive = match[1].trim();
        if (['!hide', '!show'].includes(directive)) {
            const isShow = directive === '!show';
            if (spanStart === undefined) {
                // Add a span at the start which is the reverse of the actual first directive
                ret.push({ start: 0, end: directiveStart, visible: !isShow });
            }
            else {
                // Else add a span for the current directive
                ret.push({ start: spanStart, end: directiveStart, visible });
            }
            visible = isShow;
            // A directive eats its trailing newline.
            spanStart = match.index + match[0].length + 1;
        }
    }
    // Add the remainder under the last visibility
    ret.push({ start: spanStart ?? 0, end: source.length, visible });
    // Filter empty spans and return
    return ret.filter((s) => s.start < s.end);
}
/**
 * Whether span a is fully inside span b
 */
function spanInside(a, b) {
    return b.start <= a.start && a.end <= b.end;
}
function spanContains(a, position) {
    return a.start <= position && position < a.end;
}
//# sourceMappingURL=visible-spans.js.map