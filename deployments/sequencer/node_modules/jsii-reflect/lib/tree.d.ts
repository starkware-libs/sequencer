import { AsciiTree } from 'oo-ascii-tree';
import { TypeSystem } from './type-system';
export interface TypeSystemTreeOptions {
    /**
     * Show all entity types (supersedes other options)
     * @default false
     */
    showAll?: boolean;
    /**
     * Show type members (methods, properties)
     * @default false
     */
    members?: boolean;
    /**
     * Show dependencies
     * @default false
     */
    dependencies?: boolean;
    /**
     * Show inheritance information (base classes, interfaces)
     * @default false
     */
    inheritance?: boolean;
    /**
     * Show types
     * @default false
     */
    types?: boolean;
    /**
     * Show method signatures.
     * @default false
     */
    signatures?: boolean;
    /**
     * Output with ANSI colors
     * @default true
     */
    colors?: boolean;
    /**
     * Show stabilities
     *
     * @default false
     */
    stabilities?: boolean;
}
/**
 * Visualizes a `TypeSystem` as an ASCII tree.
 */
export declare class TypeSystemTree extends AsciiTree {
    constructor(typesys: TypeSystem, options?: TypeSystemTreeOptions);
}
//# sourceMappingURL=tree.d.ts.map