import { TypeSystem } from './type-system';
/**
 * Decorates property readers for readonly properties so that their results are
 * memoized in a `WeakMap`-based cache. Those properties will consequently be
 * computed exactly once.
 *
 * This can only be applied to property accessors (`public get foo(): any`), and not to
 * property declarations (`public readonly foo: any`).
 *
 * This should not be applied to any computations relying on a typesystem.
 * The typesystem can be changed and thus change the result of the call.
 * Use `memoizedWhenLocked` instead.
 */
export declare function memoized(_prototype: unknown, propertyKey: string, descriptor: PropertyDescriptor): void;
export declare function memoizedWhenLocked<T extends {
    system: TypeSystem;
}>(_prototype: T, propertyKey: string, descriptor: PropertyDescriptor): void;
//# sourceMappingURL=_memoized.d.ts.map