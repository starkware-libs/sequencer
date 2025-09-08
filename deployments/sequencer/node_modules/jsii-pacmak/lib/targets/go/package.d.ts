import { CodeMaker } from 'codemaker';
import { Assembly, ModuleLike as JsiiModuleLike, Submodule as JsiiSubmodule } from 'jsii-reflect';
import { EmitContext } from './emit-context';
import { GoClass, GoType, GoInterface, GoTypeRef } from './types';
export declare const GOMOD_FILENAME = "go.mod";
export declare const GO_VERSION = "1.23";
export declare abstract class Package {
    private readonly jsiiModule;
    readonly packageName: string;
    readonly filePath: string;
    readonly moduleName: string;
    readonly version: string;
    readonly root: Package;
    readonly file: string;
    readonly directory: string;
    readonly submodules: InternalPackage[];
    readonly types: GoType[];
    private readonly embeddedTypes;
    private readonly readmeFile?;
    constructor(jsiiModule: JsiiModuleLike, packageName: string, filePath: string, moduleName: string, version: string, root?: Package);
    get dependencies(): Package[];
    get goModuleName(): string;
    findType(fqn: string): GoType | undefined;
    emit(context: EmitContext): void;
    emitSubmodules(context: EmitContext): void;
    /**
     * Determines if `type` comes from a foreign package.
     */
    isExternalType(type: GoClass | GoInterface): boolean;
    /**
     * Returns the name of the embed field used to embed a base class/interface in a
     * struct.
     *
     * @returns If the base is in the same package, returns the proxy name of the
     * base under `embed`, otherwise returns a unique symbol under `embed` and the
     * original interface reference under `original`.
     *
     * @param type The base type we want to embed
     */
    resolveEmbeddedType(type: GoClass | GoInterface): EmbeddedType;
    protected emitHeader(code: CodeMaker): void;
    /**
     * Emits a `func init() { ... }` in a dedicated file (so we don't have to
     * worry about what needs to be imported and whatnot). This function is
     * responsible for correctly initializing the module, including registering
     * the declared types with the jsii runtime for go.
     */
    private emitGoInitFunction;
    private emitImports;
    private emitTypes;
    private emitValidators;
    private emitInternal;
}
export declare class RootPackage extends Package {
    readonly assembly: Assembly;
    readonly version: string;
    private readonly versionFile;
    private readonly typeCache;
    private readonly rootPackageCache;
    constructor(assembly: Assembly, rootPackageCache?: Map<string, RootPackage>);
    emit(context: EmitContext): void;
    private emitGomod;
    findType(fqn: string): GoType | undefined;
    get packageDependencies(): RootPackage[];
    protected emitHeader(code: CodeMaker): void;
    private emitJsiiPackage;
}
export declare class InternalPackage extends Package {
    readonly parent: Package;
    constructor(root: Package, parent: Package, assembly: JsiiSubmodule);
}
/**
 * Represents an embedded Go type.
 */
interface EmbeddedType {
    /**
     * The field name for the embedded type.
     */
    readonly fieldName: string;
    /**
     * The embedded type name to use. Could be either a struct proxy (if the base
     * type is in the same package) or an internal alias for a foriegn type name.
     */
    readonly embed: string;
    /**
     * Refernce to the foreign type (if this is a foriegn type)
     */
    readonly foriegnType?: GoTypeRef;
    /**
     * The name of the foriegn type.
     */
    readonly foriegnTypeName?: string;
}
export {};
//# sourceMappingURL=package.d.ts.map