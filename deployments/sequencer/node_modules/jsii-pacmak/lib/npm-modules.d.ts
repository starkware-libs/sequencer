import { JsiiModule } from './packaging';
import { Toposorted } from './toposort';
/**
 * Find all modules that need to be packagerd
 *
 * If the input list is empty, include the current directory.
 *
 * The result is topologically sorted.
 */
export declare function findJsiiModules(directories: readonly string[], recurse: boolean): Promise<Toposorted<JsiiModule>>;
export declare function updateAllNpmIgnores(packages: JsiiModule[]): Promise<void>;
//# sourceMappingURL=npm-modules.d.ts.map