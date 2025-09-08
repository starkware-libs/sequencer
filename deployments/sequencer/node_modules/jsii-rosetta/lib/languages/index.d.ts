import { TargetLanguage } from './target-language';
import { VisualizeAstVisitor } from './visualize';
import { AstHandler } from '../renderer';
export { TargetLanguage };
export interface VisitorFactory {
    readonly version: string;
    createVisitor(): AstHandler<any>;
}
export declare const TARGET_LANGUAGES: {
    [key in TargetLanguage]: VisitorFactory;
};
export declare function getVisitorFromLanguage(language: string | undefined): VisualizeAstVisitor | AstHandler<any>;
//# sourceMappingURL=index.d.ts.map