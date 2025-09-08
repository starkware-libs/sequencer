import * as jsii from '@jsii/spec';
import { Callable } from './callable';
import { Docs, Documentable } from './docs';
import { OptionalValue } from './optional-value';
import { Type } from './type';
import { TypeSystem } from './type-system';
export declare class Parameter extends OptionalValue implements Documentable {
    readonly parentType: Type;
    readonly method: Callable;
    readonly spec: jsii.Parameter;
    constructor(system: TypeSystem, parentType: Type, method: Callable, spec: jsii.Parameter);
    /**
     * The name of the parameter.
     */
    get name(): string;
    /**
     * Whether this argument is the "rest" of a variadic signature.
     * The ``#type`` is that of every individual argument of the variadic list.
     */
    get variadic(): boolean;
    get docs(): Docs;
}
//# sourceMappingURL=parameter.d.ts.map