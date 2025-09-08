/**
 * All the visitors in this module expose CommonMark types in their API
 *
 * We want to keep CommonMark as a private dependency (so we don't have to
 * mark it as peerDependency and can keep its @types in devDependencies),
 * so we re-expose the main functionality needed by pacmak as functions
 * that operate on basic types here.
 */
export declare function markDownToStructure(source: string): string;
export declare function markDownToJavaDoc(source: string): string;
export declare function markDownToXmlDoc(source: string): string;
//# sourceMappingURL=index.d.ts.map