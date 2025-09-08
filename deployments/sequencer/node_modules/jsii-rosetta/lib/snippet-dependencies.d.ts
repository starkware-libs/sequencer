import { PackageJson } from '@jsii/spec';
import { TypeScriptSnippet, CompilationDependency } from './snippet';
/**
 * Collect the dependencies of a bunch of snippets together in one declaration
 *
 * We assume here the dependencies will not conflict.
 */
export declare function collectDependencies(snippets: TypeScriptSnippet[]): Record<string, CompilationDependency>;
/**
 * Add transitive dependencies of concrete dependencies to the array
 *
 * This is necessary to prevent multiple copies of transitive dependencies on disk, which
 * jsii-based packages might not deal with very well.
 */
export declare function expandWithTransitiveDependencies(deps: Record<string, CompilationDependency>): Promise<void>;
/**
 * Find the corresponding package directories for all dependencies in a package.json
 */
export declare function resolveDependenciesFromPackageJson(packageJson: PackageJson | undefined, directory: string): Promise<Record<string, {
    readonly type: "concrete";
    readonly resolvedDirectory: string;
}>>;
/**
 * Check that the directory we were given has all the necessary dependencies in it
 *
 * It's a warning if this is not true, not an error.
 */
export declare function validateAvailableDependencies(directory: string, deps: Record<string, CompilationDependency>): Promise<void>;
/**
 * Prepare a temporary directory with symlinks to all the dependencies we need.
 *
 * - Symlinks the concrete dependencies
 * - Tries to first find the symbolic dependencies in a potential monorepo that might be present
 *   (try both `lerna` and `yarn` monorepos).
 * - Installs the remaining symbolic dependencies using 'npm'.
 */
export declare function prepareDependencyDirectory(deps: Record<string, CompilationDependency>): Promise<string>;
//# sourceMappingURL=snippet-dependencies.d.ts.map