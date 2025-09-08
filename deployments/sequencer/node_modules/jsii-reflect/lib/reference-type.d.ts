import * as jsii from '@jsii/spec';
import { Assembly } from './assembly';
import { InterfaceType } from './interface';
import { Method } from './method';
import { Property } from './property';
import { Type } from './type';
import { TypeMember } from './type-member';
import { TypeSystem } from './type-system';
export declare abstract class ReferenceType extends Type {
    system: TypeSystem;
    assembly: Assembly;
    constructor(system: TypeSystem, assembly: Assembly, spec: jsii.Type);
    /**
     * All the base interfaces that this interface extends.
     */
    get interfaces(): InterfaceType[];
    /**
     * List of methods (without inherited methods).
     */
    get ownMethods(): Method[];
    /**
     * List of own and inherited methods
     */
    get allMethods(): Method[];
    /**
     * List of properties.
     */
    get ownProperties(): Property[];
    /**
     * List of own and inherited methods
     */
    get allProperties(): Property[];
    get ownMembers(): TypeMember[];
    get allMembers(): TypeMember[];
    getMembers(inherited?: boolean): {
        [name: string]: TypeMember;
    };
    /**
     * Lists all interfaces this interface extends.
     * @param inherited include all interfaces implemented by all super interfaces (default: false)
     */
    abstract getInterfaces(inherited?: boolean): InterfaceType[];
    /**
     * Lists all properties in this class.
     * @param inherited include all properties inherited from base classes (default: false)
     */
    abstract getProperties(inherited?: boolean): {
        [name: string]: Property;
    };
    /**
     * List all methods in this class.
     * @param inherited include all methods inherited from base classes (default: false)
     */
    abstract getMethods(inherited?: boolean): {
        [name: string]: Method;
    };
}
//# sourceMappingURL=reference-type.d.ts.map