import * as jsii from '@jsii/spec';
import { TypeReference } from './type-ref';
import { TypeSystem } from './type-system';
export declare class OptionalValue {
    readonly system: TypeSystem;
    readonly spec?: jsii.OptionalValue | undefined;
    static describe(optionalValue: OptionalValue): string;
    constructor(system: TypeSystem, spec?: jsii.OptionalValue | undefined);
    toString(): string;
    get type(): TypeReference;
    get optional(): boolean;
}
//# sourceMappingURL=optional-value.d.ts.map