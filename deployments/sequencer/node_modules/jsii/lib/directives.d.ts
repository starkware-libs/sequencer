import * as ts from 'typescript';
import { JsiiDiagnostic } from './jsii-diagnostic';
/**
 * TSDoc-style directives that can be attached to a symbol.
 */
export declare class Directives {
    #private;
    /**
     * Obtains the `Directives` for a given TypeScript AST node.
     *
     * @param node         the node for which directives are requested.
     * @param onDiagnostic a callback invoked whenever a diagnostic message is
     *                     emitted when parsing directives.
     */
    static of(node: ts.Node, onDiagnostic: (diag: JsiiDiagnostic) => void): Directives;
    /** Whether the node has the `@jsii ignore` directive set. */
    readonly ignore?: ts.JSDocComment | ts.JSDocTag;
    /** Whether the node has the `@jsii struct` directive set. */
    readonly struct?: ts.JSDocComment | ts.JSDocTag;
    private constructor();
}
//# sourceMappingURL=directives.d.ts.map