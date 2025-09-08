import { Assembly, Submodule } from 'jsii-reflect';
import { Package } from './package';
import { GoMethod, GoTypeMember, GoType } from './types';
export declare function findTypeInTree(module: Package, fqn: string): GoType | undefined;
export declare function goPackageNameForAssembly(assembly: Assembly | Submodule): string;
export declare function flatMap<T, R>(collection: readonly T[], mapper: (value: T) => readonly R[]): readonly R[];
export declare function getMemberDependencies(members: readonly GoTypeMember[]): Package[];
export declare function getParamDependencies(methods: readonly GoMethod[]): Package[];
export declare function substituteReservedWords(name: string): string;
/**
 * Computes a safe tarball name for the provided assembly.
 *
 * @param assm the assembly.
 *
 * @returns a tarball name.
 */
export declare function tarballName(assm: Assembly): string;
//# sourceMappingURL=util.d.ts.map