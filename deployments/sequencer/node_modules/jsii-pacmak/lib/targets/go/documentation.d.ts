import { CodeMaker } from 'codemaker';
import { Docs } from 'jsii-reflect';
import { ApiLocation, RosettaTabletReader } from 'jsii-rosetta';
export declare class Documentation {
    private readonly code;
    private readonly rosetta;
    constructor(code: CodeMaker, rosetta: RosettaTabletReader);
    /**
     * Emits all documentation depending on what is available in the jsii model
     *
     * Used by all kind of members + classes, interfaces, enums
     * Order should be
     * Summary + Remarks
     * Returns
     * Examples <transliterated example code>
     * Link
     * Stability/Deprecation description
     */
    emit(docs: Docs, apiLocation: ApiLocation): void;
    emitStability(docs: Docs): void;
    emitReadme(moduleFqn: string, readme: string, directory: string): void;
    private emitComment;
    private shouldMentionStability;
}
//# sourceMappingURL=documentation.d.ts.map