"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.SnippetParameters = exports.INITIALIZER_METHOD_NAME = void 0;
exports.formatLocation = formatLocation;
exports.renderApiLocation = renderApiLocation;
exports.typeScriptSnippetFromVisibleSource = typeScriptSnippetFromVisibleSource;
exports.typeScriptSnippetFromSource = typeScriptSnippetFromSource;
exports.typeScriptSnippetFromCompleteSource = typeScriptSnippetFromCompleteSource;
exports.updateParameters = updateParameters;
exports.completeSource = completeSource;
exports.parseKeyValueList = parseKeyValueList;
exports.parseMetadataLine = parseMetadataLine;
exports.renderMetadataline = renderMetadataline;
const visible_spans_1 = require("./typescript/visible-spans");
/**
 * How to represent the initializer in a 'parameter' type.
 *
 * (Don't feel like making everyone's `case` statement worse by adding an
 * 'initializer-parameter' variant).
 */
exports.INITIALIZER_METHOD_NAME = '<initializer>';
/**
 * Render an API location to a human readable representation
 */
function formatLocation(location) {
    switch (location.field?.field) {
        case 'example':
            return `${renderApiLocation(location.api)}-example`;
        case 'markdown':
            return `${renderApiLocation(location.api)}-L${location.field.line}`;
        case undefined:
            return renderApiLocation(location.api);
    }
}
/**
 * Render an API location to an unique string
 *
 * This function is used in hashing examples for reuse, and so the formatting
 * here should not be changed lightly.
 */
function renderApiLocation(apiLoc) {
    switch (apiLoc.api) {
        case 'file':
            return apiLoc.fileName;
        case 'moduleReadme':
            return `${apiLoc.moduleFqn}-README`;
        case 'type':
            return apiLoc.fqn;
        case 'initializer':
            return `${apiLoc.fqn}#initializer`;
        case 'member':
            return `${apiLoc.fqn}#${apiLoc.memberName}`;
        case 'parameter':
            return `${apiLoc.fqn}#${apiLoc.methodName}!#${apiLoc.parameterName}`;
    }
}
/**
 * Construct a TypeScript snippet from visible source
 *
 * Will parse parameters from a directive in the given source, but will not
 * interpret `/// !show` and `/// !hide` directives.
 *
 * `/// !show` and `/// !hide` directives WILL affect what gets displayed by
 * the translator, but they will NOT affect the snippet's cache key (i.e. the
 * cache key will be based on the full source given here).
 *
 * Use this if you are looking up a snippet in a tablet, which has been translated
 * previously using a fixture.
 */
function typeScriptSnippetFromVisibleSource(typeScriptSource, location, strict, parameters = {}) {
    const [source, sourceParameters] = parametersFromSourceDirectives(typeScriptSource);
    const visibleSource = source.trimEnd();
    return {
        visibleSource,
        location,
        parameters: Object.assign({}, parameters, sourceParameters),
        strict,
    };
}
/**
 * Construct a TypeScript snippet from literal source
 *
 * @deprecated Use `typeScriptSnippetFromVisibleSource`
 */
function typeScriptSnippetFromSource(typeScriptSource, location, strict, parameters = {}) {
    return typeScriptSnippetFromVisibleSource(typeScriptSource, location, strict, parameters);
}
/**
 * Construct a TypeScript snippet from complete source
 *
 * Will parse parameters from a directive in the given source, and will
 * interpret `/// !show` and `/// !hide` directives.
 *
 * The snippet's cache key will be based on the source that remains after
 * these directives are processed.
 *
 * Use this if you are building a snippet to be translated, and take care
 * to store the return object's `visibleSource` in the assembly (not the original
 * source you passed in).
 */
function typeScriptSnippetFromCompleteSource(typeScriptSource, location, strict, parameters = {}) {
    const [source, sourceParameters] = parametersFromSourceDirectives(typeScriptSource);
    const completeSrc = source.trimRight();
    const visibleSource = (0, visible_spans_1.trimCompleteSourceToVisible)(completeSrc);
    return {
        visibleSource,
        completeSource: visibleSource !== completeSrc ? completeSrc : undefined,
        location,
        parameters: Object.assign({}, parameters, sourceParameters),
        strict,
    };
}
function updateParameters(snippet, params) {
    return {
        ...snippet,
        parameters: Object.assign(Object.create(null), snippet.parameters ?? {}, params),
    };
}
/**
 * Get the complete (compilable) source of a snippet
 */
function completeSource(snippet) {
    return snippet.completeSource ?? snippet.visibleSource;
}
/**
 * Extract snippet parameters from the first line of the source if it's a compiler directive
 */
function parametersFromSourceDirectives(source) {
    const [firstLine, ...rest] = source.split('\n');
    // Also extract parameters from an initial line starting with '/// ' (getting rid of that line).
    const m = /[/]{3}(.*)$/.exec(firstLine);
    if (m) {
        return [rest.join('\n'), parseMetadataLine(m[1])];
    }
    return [source, {}];
}
/**
 * Parse a set of 'param param=value' directives into an object
 */
function parseKeyValueList(parameters) {
    const ret = {};
    for (const param of parameters) {
        const parts = param.split('=', 2);
        if (parts.length === 2) {
            ret[parts[0]] = parts[1];
        }
        else {
            ret[parts[0]] = '';
        }
    }
    return ret;
}
function parseMetadataLine(metadata) {
    return parseKeyValueList(parseMetadata(metadata));
    function parseMetadata(md) {
        return md
            .trim()
            .split(' ')
            .map((s) => s.trim())
            .filter((s) => s !== '');
    }
}
function renderMetadataline(metadata = {}) {
    const line = Object.entries(metadata)
        .filter(([key, _]) => !key.startsWith('$'))
        .map(([key, value]) => (value !== '' ? `${key}=${value}` : key))
        .join(' ');
    return line ? line : undefined;
}
/**
 * Recognized snippet parameters
 */
var SnippetParameters;
(function (SnippetParameters) {
    /**
     * Use fixture with the given name (author parameter)
     */
    SnippetParameters["FIXTURE"] = "fixture";
    /**
     * Don't use a fixture (author parameter)
     */
    SnippetParameters["NO_FIXTURE"] = "nofixture";
    /**
     * Snippet was extracted from this literate file (backwards compatibility)
     *
     * Parameter attached by 'jsii'; load the given file instead of any fixture,
     * process as usual.
     */
    SnippetParameters["LITERATE_SOURCE"] = "lit";
    /**
     * This snippet has been infused
     *
     * This means it has been copied from a different location, and potentially
     * even from a different assembly. If so, we can't expect it to compile in
     * the future, and if doesn't, we ignore the errors.
     *
     * N.B: this shouldn't make a difference in normal operation, as the `infuse`
     * command will duplicate the translation to the target tablet. This only
     * matters if we remove the tablet and try to re-extract an assembly with
     * infused examples from somewher else.
     */
    SnippetParameters["INFUSED"] = "infused";
    /**
     * What directory to resolve fixtures in for this snippet (system parameter)
     *
     * Attached during processing, should not be used by authors. Does NOT imply
     * anything about the directory where we pretend to compile this file.
     */
    SnippetParameters["$PROJECT_DIRECTORY"] = "$directory";
    /**
     * What directory to pretend the file is in (system parameter)
     *
     * Attached when compiling a literate file, as they compile in
     * the location where they are stored.
     */
    SnippetParameters["$COMPILATION_DIRECTORY"] = "$compilation";
})(SnippetParameters || (exports.SnippetParameters = SnippetParameters = {}));
//# sourceMappingURL=snippet.js.map