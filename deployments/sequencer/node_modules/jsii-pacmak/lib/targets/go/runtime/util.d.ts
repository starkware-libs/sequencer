import { CodeMaker } from 'codemaker';
export declare function emitInitialization(code: CodeMaker): void;
/**
 * Slugify a name by appending '_' at the end until the resulting name is not
 * present in the list of reserved names.
 *
 * @param name     the name to be slugified
 * @param reserved the list of names that are already sued in-scope
 *
 * @returns the slugified name
 */
export declare function slugify(name: string, reserved: Iterable<string>): string;
//# sourceMappingURL=util.d.ts.map