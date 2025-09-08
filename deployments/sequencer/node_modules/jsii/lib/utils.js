"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.JsiiError = exports.JSII_DIAGNOSTICS_CODE = exports.DIAGNOSTICS = exports.CLI_LOGGER = void 0;
exports.diagnosticsLogger = diagnosticsLogger;
exports.formatDiagnostic = formatDiagnostic;
exports._formatDiagnostic = _formatDiagnostic;
exports.logDiagnostic = logDiagnostic;
exports.parsePerson = parsePerson;
exports.parseRepository = parseRepository;
exports.stripAnsi = stripAnsi;
const log4js = require("log4js");
const ts = require("typescript");
const jsii_diagnostic_1 = require("./jsii-diagnostic");
/**
 * Name of the logger for cli errors
 */
exports.CLI_LOGGER = 'jsii/cli';
/**
 * Name of the logger for diagnostics information
 */
exports.DIAGNOSTICS = 'diagnostics';
/**
 * Diagnostic code for JSII-generated messages.
 */
exports.JSII_DIAGNOSTICS_CODE = 9999;
/**
 * Obtains the relevant logger to be used for a given diagnostic message.
 *
 * @param logger     the ``log4js.Logger`` to use for emitting the message.
 * @param diagnostic the message for which a logger is requested.
 *
 * @returns a logger method of the ``logger`` for the appropriate level.
 */
function diagnosticsLogger(logger, diagnostic) {
    switch (diagnostic.category) {
        case ts.DiagnosticCategory.Error:
            if (!logger.isErrorEnabled()) {
                return undefined;
            }
            return logger.error.bind(logger);
        case ts.DiagnosticCategory.Warning:
            if (!logger.isWarnEnabled()) {
                return undefined;
            }
            return logger.warn.bind(logger);
        case ts.DiagnosticCategory.Message:
            if (!logger.isDebugEnabled()) {
                return undefined;
            }
            return logger.debug.bind(logger);
        case ts.DiagnosticCategory.Suggestion:
        default:
            if (!logger.isTraceEnabled()) {
                return undefined;
            }
            return logger.trace.bind(logger);
    }
}
/**
 * Formats a diagnostic message with color and context, if possible.
 *
 * @param diagnostic  the diagnostic message ot be formatted.
 * @param projectRoot the root of the TypeScript project.
 *
 * @returns a formatted string.
 */
function formatDiagnostic(diagnostic, projectRoot) {
    if (jsii_diagnostic_1.JsiiDiagnostic.isJsiiDiagnostic(diagnostic)) {
        // Ensure we leverage pre-rendered diagnostics where available.
        return diagnostic.format(projectRoot);
    }
    return _formatDiagnostic(diagnostic, projectRoot);
}
/**
 * Formats a diagnostic message with color and context, if possible. Users
 * should use `formatDiagnostic` instead, as this implementation is intended for
 * internal usafe only.
 *
 * @param diagnostic  the diagnostic message ot be formatted.
 * @param projectRoot the root of the TypeScript project.
 *
 * @returns a formatted string.
 */
function _formatDiagnostic(diagnostic, projectRoot) {
    const formatDiagnosticsHost = {
        getCurrentDirectory: () => projectRoot,
        getCanonicalFileName: (fileName) => fileName,
        getNewLine: () => ts.sys.newLine,
    };
    const message = diagnostic.file != null
        ? ts.formatDiagnosticsWithColorAndContext([diagnostic], formatDiagnosticsHost)
        : ts.formatDiagnostic(diagnostic, formatDiagnosticsHost);
    if (!jsii_diagnostic_1.JsiiDiagnostic.isJsiiDiagnostic(diagnostic)) {
        return message;
    }
    // This is our own diagnostics, so we'll format appropriately (replacing TS#### with JSII####).
    return message.replace(` TS${exports.JSII_DIAGNOSTICS_CODE}: `, ` JSII${diagnostic.jsiiCode}: `);
}
function logDiagnostic(diagnostic, projectRoot) {
    const logFunc = diagnosticsLogger(log4js.getLogger(exports.DIAGNOSTICS), diagnostic);
    if (!logFunc) {
        return;
    }
    logFunc(formatDiagnostic(diagnostic, projectRoot).trim());
}
const PERSON_REGEX = /^\s*(.+?)(?:\s*<([^>]+)>)?(?:\s*\(([^)]+)\))?\s*$/;
/**
 * Parses a string-formatted person entry from `package.json`.
 * @param value the string-formatted person entry.
 *
 * @example
 *  parsePerson("Barney Rubble <b@rubble.com> (http://barnyrubble.tumblr.com/)");
 *  // => { name: "Barney Rubble", email: "b@rubble.com", url: "http://barnyrubble.tumblr.com/" }
 */
function parsePerson(value) {
    const match = PERSON_REGEX.exec(value);
    if (!match) {
        throw new JsiiError(`Invalid stringified "person" value: ${value}`);
    }
    const [, name, email, url] = match;
    const result = {
        name: name.trim(),
    };
    if (email) {
        result.email = email.trim();
    }
    if (url) {
        result.url = url.trim();
    }
    return result;
}
const REPOSITORY_REGEX = /^(?:(github|gist|bitbucket|gitlab):)?([A-Za-z\d_-]+\/[A-Za-z\d_-]+)$/;
function parseRepository(value) {
    const match = REPOSITORY_REGEX.exec(value);
    if (!match) {
        return { url: value };
    }
    const [, host, slug] = match;
    switch (host ?? 'github') {
        case 'github':
            return { url: `https://github.com/${slug}.git` };
        case 'gist':
            return { url: `https://gist.github.com/${slug}.git` };
        case 'bitbucket':
            return { url: `https://bitbucket.org/${slug}.git` };
        case 'gitlab':
            return { url: `https://gitlab.com/${slug}.git` };
        default:
            throw new JsiiError(`Unknown repository hosting service: ${host}`);
    }
}
const ANSI_REGEX = 
// eslint-disable-next-line no-control-regex
/[\u001b\u009b][[()#;?]*(?:[0-9]{1,4}(?:;[0-9]{0,4})*)?[0-9A-ORZcf-nqry=><]/g;
function stripAnsi(x) {
    return x.replace(ANSI_REGEX, '');
}
/**
 * Throws an error that is intended as CLI output.
 */
class JsiiError extends Error {
    /**
     * An expected error that can be nicely formatted where needed (e.g. in CLI output)
     * This should only be used for errors that a user can fix themselves.
     *
     * @param message The error message to be printed to the user.
     * @param showHelp Print the help before the error.
     */
    constructor(message, showHelp = false) {
        super(message);
        this.message = message;
        this.showHelp = showHelp;
        Object.setPrototypeOf(this, JsiiError.prototype);
    }
}
exports.JsiiError = JsiiError;
//# sourceMappingURL=utils.js.map