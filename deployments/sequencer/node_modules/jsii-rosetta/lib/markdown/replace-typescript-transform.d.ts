import { ReplaceCodeTransform } from './replace-code-renderer';
import { CodeBlock } from './types';
import { TypeScriptSnippet, ApiLocation } from '../snippet';
export type TypeScriptReplacer = (code: TypeScriptSnippet) => CodeBlock | undefined;
/**
 * A specialization of ReplaceCodeTransform that maintains state about TypeScript snippets
 */
export declare class ReplaceTypeScriptTransform extends ReplaceCodeTransform {
    constructor(api: ApiLocation, strict: boolean, replacer: TypeScriptReplacer);
}
//# sourceMappingURL=replace-typescript-transform.d.ts.map