import { CustomTransformers } from 'typescript';
/**
 * Combines a collection of `CustomTransformers` configurations into a single
 * one, preserving the order of arguments.
 *
 * @param transformers the list of transformers to combine.
 *
 * @returns the combined transformer.
 */
export declare function combinedTransformers(...transformers: readonly CustomTransformers[]): CustomTransformers;
//# sourceMappingURL=utils.d.ts.map