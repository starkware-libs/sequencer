import { TypeReference } from 'jsii-reflect';
import { SpecialDependencies } from '../dependencies';
import { Package } from '../package';
import { GoType } from './go-type';
/**
 * TypeMap used to recursively resolve interfaces in nested types for use in
 * resolving scoped type names and implementation maps.
 */
type TypeMap = {
    readonly type: 'primitive';
    readonly value: string;
} | {
    readonly type: 'array';
    readonly value: GoTypeRef;
} | {
    readonly type: 'map';
    readonly value: GoTypeRef;
} | {
    readonly type: 'union';
    readonly value: readonly GoTypeRef[];
} | {
    readonly type: 'interface';
    readonly value: GoTypeRef;
} | {
    readonly type: 'void';
};
export declare class GoTypeRef {
    readonly root: Package;
    readonly reference: TypeReference;
    private readonly options;
    private _typeMap?;
    constructor(root: Package, reference: TypeReference, options?: {
        readonly opaqueUnionTypes: boolean;
    });
    get type(): GoType | undefined;
    get specialDependencies(): SpecialDependencies;
    get primitiveType(): string | undefined;
    get name(): string | undefined;
    get datatype(): boolean | undefined;
    get namespace(): string | undefined;
    get void(): boolean;
    get typeMap(): TypeMap;
    /**
     * The go `import`s required in order to be able to use this type in code.
     */
    get dependencies(): readonly Package[];
    get unionOfTypes(): readonly GoTypeRef[] | undefined;
    get withTransparentUnions(): GoTypeRef;
    scopedName(scope: Package): string;
    scopedReference(scope: Package): string;
    private buildTypeMap;
    scopedTypeName(typeMap: TypeMap, scope: Package, asRef?: boolean): string;
}
export {};
//# sourceMappingURL=go-type-reference.d.ts.map