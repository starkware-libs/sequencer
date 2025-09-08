import * as ts from 'typescript';
import { RosettaDiagnostic } from './translate';
export declare function startsWithUppercase(x: string): boolean;
export interface File {
    readonly contents: string;
    readonly fileName: string;
}
export declare function printDiagnostics(diags: readonly RosettaDiagnostic[], stream: NodeJS.WritableStream, colors: boolean): void;
export declare function formatList(xs: string[], n?: number): string;
export declare const StrictBrand = "jsii.strict";
/**
 * Annotate a diagnostic with a magic property to indicate it's a strict diagnostic
 */
export declare function annotateStrictDiagnostic(diag: ts.Diagnostic): void;
/**
 * Return whether or not the given diagnostic was annotated with the magic strict property
 */
export declare function hasStrictBranding(diag: ts.Diagnostic): boolean;
/**
 * Chunk an array of elements into approximately equal groups
 */
export declare function divideEvenly<A>(groups: number, xs: A[]): A[][];
export declare function flat<A>(xs: A[][]): A[];
/**
 * Partition a list in twain using a predicate
 *
 * Returns [elements-matching-predicate, elements-not-matching-predicate];
 */
export declare function partition<A>(xs: A[], pred: (x: A) => boolean): [A[], A[]];
export declare function setExtend<A>(xs: Set<A>, els: Iterable<A>): void;
export declare function mkDict<A extends string, B>(xs: Array<readonly [A, B]>): Record<A, B>;
/**
 * Apply a function to a value, as long as it's not `undefined`
 *
 * This is a companion helper to TypeScript's nice `??` and `?.` nullish
 * operators. Those operators are helpful if you're calling methods:
 *
 *    object?.method()  <- returns 'undefined' if 'object' is nullish
 *
 * But are no help when you want to use free functions:
 *
 *    func(object)      <- but what if 'object' is nullish and func
 *                         expects it not to be?
 *
 * Yes you can write `object ? func(object) : undefined` but the trailing
 * `: undefined` clutters your code. Instead, you write:
 *
 *    fmap(object, func)
 *
 * The name `fmap` is taken from Haskell: it's a "Functor-map" (although
 * only for the `Maybe` Functor).
 */
export declare function fmap<A, B>(value: NonNullable<A>, fn: (x: NonNullable<A>) => B): B;
export declare function fmap<A, B>(value: undefined | null, fn: (x: NonNullable<A>) => B): undefined;
export declare function fmap<A, B>(value: A | undefined | null, fn: (x: A) => B): B | undefined;
export declare function mapValues<A, B>(xs: Record<string, A>, fn: (x: A) => B): Record<string, B>;
/**
 * Sort an array by a key function.
 *
 * Instead of having to write your own comparators for your types any time you
 * want to sort, you supply a function that maps a value to a compound sort key
 * consisting of numbers or strings. The sorting will happen by that sort key
 * instead.
 */
export declare function sortBy<A>(xs: A[], keyFn: (x: A) => Array<string | number>): A[];
/**
 * Group elements by a key
 *
 * Supply a function that maps each element to a key string.
 *
 * Returns a map of the key to the list of elements that map to that key.
 */
export declare function groupBy<A>(xs: A[], keyFn: (x: A) => string): Record<string, A[]>;
export declare function isDefined<A>(x: A): x is NonNullable<A>;
export declare function indexBy<A>(xs: A[], fn: (x: A) => string): Record<string, A>;
export type Mutable<T> = {
    -readonly [P in keyof T]: Mutable<T[P]>;
};
export declare function commentToken(language: string): "#" | "//";
export declare function pathExists(path: string): Promise<boolean>;
//# sourceMappingURL=util.d.ts.map