import { CodeBlock } from './types';
import { TypeScriptSnippet, ApiLocation } from '../snippet';
export type TypeScriptReplacer = (code: TypeScriptSnippet) => CodeBlock | undefined;
export declare function extractTypescriptSnippetsFromMarkdown(markdown: string, location: ApiLocation, strict: boolean): TypeScriptSnippet[];
//# sourceMappingURL=extract-snippets.d.ts.map