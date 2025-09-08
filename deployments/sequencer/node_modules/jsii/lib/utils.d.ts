import * as log4js from 'log4js';
import * as ts from 'typescript';
/**
 * Name of the logger for cli errors
 */
export declare const CLI_LOGGER = "jsii/cli";
/**
 * Name of the logger for diagnostics information
 */
export declare const DIAGNOSTICS = "diagnostics";
/**
 * Diagnostic code for JSII-generated messages.
 */
export declare const JSII_DIAGNOSTICS_CODE = 9999;
/**
 * Obtains the relevant logger to be used for a given diagnostic message.
 *
 * @param logger     the ``log4js.Logger`` to use for emitting the message.
 * @param diagnostic the message for which a logger is requested.
 *
 * @returns a logger method of the ``logger`` for the appropriate level.
 */
export declare function diagnosticsLogger(logger: log4js.Logger, diagnostic: ts.Diagnostic): ((message: any, ...args: any[]) => void) | undefined;
/**
 * Formats a diagnostic message with color and context, if possible.
 *
 * @param diagnostic  the diagnostic message ot be formatted.
 * @param projectRoot the root of the TypeScript project.
 *
 * @returns a formatted string.
 */
export declare function formatDiagnostic(diagnostic: ts.Diagnostic, projectRoot: string): string;
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
export declare function _formatDiagnostic(diagnostic: ts.Diagnostic, projectRoot: string): string;
export declare function logDiagnostic(diagnostic: ts.Diagnostic, projectRoot: string): void;
/**
 * Parses a string-formatted person entry from `package.json`.
 * @param value the string-formatted person entry.
 *
 * @example
 *  parsePerson("Barney Rubble <b@rubble.com> (http://barnyrubble.tumblr.com/)");
 *  // => { name: "Barney Rubble", email: "b@rubble.com", url: "http://barnyrubble.tumblr.com/" }
 */
export declare function parsePerson(value: string): {
    name: string;
    email?: string;
    url?: string;
};
export declare function parseRepository(value: string): {
    url: string;
};
export declare function stripAnsi(x: string): string;
/**
 * Maps the provided type to stip all `readonly` modifiers from its properties.
 */
export type Mutable<T> = {
    -readonly [K in keyof T]: Mutable<T[K]>;
};
/**
 * Throws an error that is intended as CLI output.
 */
export declare class JsiiError extends Error {
    readonly message: string;
    readonly showHelp: boolean;
    /**
     * An expected error that can be nicely formatted where needed (e.g. in CLI output)
     * This should only be used for errors that a user can fix themselves.
     *
     * @param message The error message to be printed to the user.
     * @param showHelp Print the help before the error.
     */
    constructor(message: string, showHelp?: boolean);
}
//# sourceMappingURL=utils.d.ts.map