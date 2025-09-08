import * as spec from '@jsii/spec';
import { CodeMaker } from 'codemaker';
import { RosettaTabletReader, ApiLocation } from 'jsii-rosetta';
/**
 * Generates the Jsii attributes and calls for the .NET runtime
 *
 * Uses the same instance of CodeMaker as the rest of the code
 */
export declare class DotNetDocGenerator {
    private readonly rosetta;
    private readonly assembly;
    private readonly code;
    private readonly nameutils;
    constructor(code: CodeMaker, rosetta: RosettaTabletReader, assembly: spec.Assembly);
    /**
     * Emits all documentation depending on what is available in the jsii model
     *
     * Used by all kind of members + classes, interfaces, enums
     * Order should be
     * Summary
     * Param
     * Returns
     * Remarks (includes examples, links, deprecated)
     */
    emitDocs(obj: spec.Documentable, apiLocation: ApiLocation): void;
    emitMarkdownAsRemarks(markdown: string | undefined, apiLocation: ApiLocation): void;
    /**
     * Returns the lines that should go into the <remarks> section {@link http://www.google.com|Google}
     */
    private renderRemarks;
    private convertExample;
    private convertSamplesInMarkdown;
    private emitXmlDoc;
}
//# sourceMappingURL=dotnetdocgenerator.d.ts.map