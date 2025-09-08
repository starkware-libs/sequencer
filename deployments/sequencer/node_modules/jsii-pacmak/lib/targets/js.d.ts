import * as spec from '@jsii/spec';
import { Generator, Legalese } from '../generator';
import { PackageInfo, Target } from '../target';
export default class JavaScript extends Target {
    static toPackageInfos(assm: spec.Assembly): {
        [language: string]: PackageInfo;
    };
    static toNativeReference(type: spec.Type): {
        typescript: string;
        javascript?: string;
    };
    protected readonly generator: PackOnly;
    build(sourceDir: string, outDir: string): Promise<void>;
}
declare class PackOnly extends Generator {
    constructor();
    save(outdir: string, tarball: string, _: Legalese): Promise<string[]>;
    protected getAssemblyOutputDir(_mod: spec.Assembly): string;
    protected onBeginInterface(_ifc: spec.InterfaceType): void;
    protected onEndInterface(_ifc: spec.InterfaceType): void;
    protected onInterfaceMethod(_ifc: spec.InterfaceType, _method: spec.Method): void;
    protected onInterfaceMethodOverload(_ifc: spec.InterfaceType, _overload: spec.Method, _originalMethod: spec.Method): void;
    protected onInterfaceProperty(_ifc: spec.InterfaceType, _prop: spec.Property): void;
    protected onProperty(_cls: spec.ClassType, _prop: spec.Property): void;
    protected onStaticProperty(_cls: spec.ClassType, _prop: spec.Property): void;
    protected onUnionProperty(_cls: spec.ClassType, _prop: spec.Property, _union: spec.UnionTypeReference): void;
    protected onMethod(_cls: spec.ClassType, _method: spec.Method): void;
    protected onMethodOverload(_cls: spec.ClassType, _overload: spec.Method, _originalMethod: spec.Method): void;
    protected onStaticMethod(_cls: spec.ClassType, _method: spec.Method): void;
    protected onStaticMethodOverload(_cls: spec.ClassType, _overload: spec.Method, _originalMethod: spec.Method): void;
}
export {};
//# sourceMappingURL=js.d.ts.map