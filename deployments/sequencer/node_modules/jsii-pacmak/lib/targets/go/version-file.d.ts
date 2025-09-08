import { CodeMaker } from 'codemaker';
/**
 * Represents the version of the go module. This is needed because the version is not
 * available in standard go module files.
 */
export declare class VersionFile {
    private readonly version;
    private static readonly NAME;
    constructor(version: string);
    emit(code: CodeMaker): void;
}
//# sourceMappingURL=version-file.d.ts.map