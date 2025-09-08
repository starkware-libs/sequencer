export { NodeRelease } from './constants';
/**
 * Checks the current process' node runtime version against the release support
 * matrix, and issues a warning to STDERR if the current version is not fully
 * supported (i.e: it is deprecated, end-of-life, or untested).
 *
 * @param envPrefix will be prepended to environment variable names that can be
 *                  used to silence version check warnings.
 */
export declare function checkNode(envPrefix?: string): void;
//# sourceMappingURL=index.d.ts.map