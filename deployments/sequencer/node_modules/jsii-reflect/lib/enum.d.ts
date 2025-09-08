import * as jsii from '@jsii/spec';
import { Assembly } from './assembly';
import { Docs, Documentable } from './docs';
import { Type } from './type';
import { TypeSystem } from './type-system';
export declare class EnumType extends Type {
    system: TypeSystem;
    assembly: Assembly;
    readonly spec: jsii.EnumType;
    constructor(system: TypeSystem, assembly: Assembly, spec: jsii.EnumType);
    get members(): EnumMember[];
    isEnumType(): this is EnumType;
}
export declare class EnumMember implements Documentable {
    readonly enumType: EnumType;
    readonly name: string;
    readonly docs: Docs;
    constructor(enumType: EnumType, memberSpec: jsii.EnumMember);
    get system(): TypeSystem;
    get assembly(): Assembly;
}
//# sourceMappingURL=enum.d.ts.map