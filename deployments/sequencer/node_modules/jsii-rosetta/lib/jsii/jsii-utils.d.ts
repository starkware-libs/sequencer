import * as spec from '@jsii/spec';
import * as ts from 'typescript';
import { TypeLookupAssembly } from './assemblies';
import { ObjectLiteralStruct } from './jsii-types';
import { AstRenderer } from '../renderer';
export declare function isNamedLikeStruct(name: string): boolean;
export declare function analyzeStructType(typeChecker: ts.TypeChecker, type: ts.Type): ObjectLiteralStruct | false;
/**
 * Whether the given type is a protocol AND comes from jsii
 *
 * - Protocol: a TypeScript interface that is *not* a "struct" type.
 *   A.k.a. "behavioral interface".
 * - From jsii: whether the interface type is defined in and exported
 *   via a jsii assembly. There can be literal interfaces defined
 *   in an example, and they will not be mangled in the same way
 *   as a jsii interface would be.
 *
 *
 * Examples:
 *
 * ```ts
 * // isJsiiProtocolType() -> false: not a protocol
 * interface Banana {
 *   readonly arc: number;
 * }
 *
 * // isJsiiProtocolType() -> might be true: depends on whether it was defined
 * // in a jsii assembly.
 * interface IHello {
 *   sayIt(): void;
 * }
 *
 * // isJsiiProtocolType() -> false: declared to not be a protocol, even though
 * // it has the naming scheme of one
 * /**
 *  * @struct
 *  * /
 * interface IPAddress {
 *   readonly octets: number[];
 * }
 * ```
 */
export declare function isJsiiProtocolType(typeChecker: ts.TypeChecker, type: ts.Type): boolean | undefined;
export declare function hasAllFlags<A extends number>(flags: A, test: A): boolean;
export declare function hasAnyFlag<A extends number>(flags: A, test: A): boolean;
export interface StructProperty {
    name: string;
    type: ts.Type | undefined;
    questionMark: boolean;
}
export declare function propertiesOfStruct(type: ts.Type, context: AstRenderer<any>): StructProperty[];
export declare function structPropertyAcceptsUndefined(prop: StructProperty): boolean;
/**
 * A TypeScript symbol resolved to its jsii type
 */
export interface JsiiSymbol {
    /**
     * FQN of the symbol
     *
     * Is either the FQN of a type (for a type). For a membr, the FQN looks like:
     * 'type.fqn#memberName'.
     */
    readonly fqn: string;
    /**
     * What kind of symbol this is
     */
    readonly symbolType: 'module' | 'type' | 'member';
    /**
     * Assembly where the type was found
     *
     * Might be undefined if the type was FAKE from jsii (for tests)
     */
    readonly sourceAssembly?: TypeLookupAssembly;
}
export declare function lookupJsiiSymbolFromNode(typeChecker: ts.TypeChecker, node: ts.Node): JsiiSymbol | undefined;
export declare function resolveJsiiSymbolType(jsiiSymbol: JsiiSymbol): spec.Type;
/**
 * Returns the jsii FQN for a TypeScript (class or type) symbol
 *
 * TypeScript only knows the symbol NAME plus the FILE the symbol is defined
 * in. We need to extract two things:
 *
 * 1. The package name (extracted from the nearest `package.json`)
 * 2. The submodule name (...?? don't know how to get this yet)
 * 3. Any containing type names or namespace names.
 *
 * For tests, we also treat symbols in a file that has the string '/// fake-from-jsii'
 * as coming from jsii.
 */
export declare function lookupJsiiSymbol(typeChecker: ts.TypeChecker, sym: ts.Symbol): JsiiSymbol | undefined;
/**
 * If the given type is an enum literal, resolve to the enum type
 */
export declare function resolveEnumLiteral(typeChecker: ts.TypeChecker, type: ts.Type): ts.Type;
export declare function resolvedSymbolAtLocation(typeChecker: ts.TypeChecker, node: ts.Node): ts.Symbol | undefined;
export declare function parentSymbol(sym: JsiiSymbol): JsiiSymbol | undefined;
/**
 * Get the last part of a dot-separated string
 */
export declare function simpleName(x: string): string;
/**
 * Get all parts except the last of a dot-separated string
 */
export declare function namespaceName(x: string): string;
//# sourceMappingURL=jsii-utils.d.ts.map