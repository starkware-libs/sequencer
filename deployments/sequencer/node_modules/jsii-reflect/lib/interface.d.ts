import * as jsii from '@jsii/spec';
import { Assembly } from './assembly';
import { Method } from './method';
import { Property } from './property';
import { ReferenceType } from './reference-type';
import { TypeSystem } from './type-system';
export declare class InterfaceType extends ReferenceType {
    #private;
    system: TypeSystem;
    assembly: Assembly;
    readonly spec: jsii.InterfaceType;
    constructor(system: TypeSystem, assembly: Assembly, spec: jsii.InterfaceType);
    /**
     * True if this interface only contains properties. Different backends might
     * have idiomatic ways to allow defining concrete instances such interfaces.
     * For example, in Java, the generator will produce a PoJo and a builder
     * which will allow users to create a concrete object with data which
     * adheres to this interface.
     */
    get datatype(): boolean;
    /**
     * Lists all interfaces this interface extends.
     * @param inherited include all interfaces implemented by all super interfaces (default: false)
     */
    getInterfaces(inherited?: boolean): InterfaceType[];
    /**
     * Lists all properties in this class.
     * @param inherited include all properties inherited from base classes (default: false)
     */
    getProperties(inherited?: boolean): {
        [name: string]: Property;
    };
    /**
     * List all methods in this class.
     * @param inherited include all methods inherited from base classes (default: false)
     */
    getMethods(inherited?: boolean): {
        [name: string]: Method;
    };
    isDataType(): this is InterfaceType;
    isInterfaceType(): this is InterfaceType;
    private _getProperties;
    private _getMethods;
}
//# sourceMappingURL=interface.d.ts.map