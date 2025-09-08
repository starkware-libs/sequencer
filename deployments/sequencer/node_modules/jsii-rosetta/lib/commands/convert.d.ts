import { AstHandler, AstRendererOptions } from '../renderer';
import { TranslateResult } from '../translate';
import { File } from '../util';
export interface TranslateMarkdownOptions extends AstRendererOptions {
    /**
     * What language to put in the returned markdown blocks
     */
    languageIdentifier?: string;
    /**
     * Whether to operate in `strict` mode or not.
     */
    strict?: boolean;
}
export declare function translateMarkdown(markdown: File, visitor: AstHandler<any>, opts?: TranslateMarkdownOptions): TranslateResult;
//# sourceMappingURL=convert.d.ts.map