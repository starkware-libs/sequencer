import * as jsii from '@jsii/spec';
import { Assembly } from './assembly';
import { Initializer } from './initializer';
import { InterfaceType } from './interface';
import { Method } from './method';
import { Property } from './property';
import { ReferenceType } from './reference-type';
import { TypeSystem } from './type-system';
export declare class ClassType extends ReferenceType {
    readonly system: TypeSystem;
    readonly assembly: Assembly;
    readonly spec: jsii.ClassType;
    constructor(system: TypeSystem, assembly: Assembly, spec: jsii.ClassType);
    /**
     * Base class (optional).
     */
    get base(): ClassType | undefined;
    /**
     * Initializer (constructor) method.
     */
    get initializer(): Initializer | undefined;
    /**
     * Indicates if this class is an abstract class.
     */
    get abstract(): boolean;
    /**
     * Returns list of all base classes (first is the direct base and last is the top-most).
     *
     * @deprecated use ClassType.ancestors instead
     */
    getAncestors(): ClassType[];
    /**
     * Returns list of all base classes (first is the direct base and last is the top-most).
     */
    get ancestors(): ClassType[];
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
    /**
     * Lists all interfaces this class implements.
     * @param inherited include all interfaces implemented by all base classes (default: false)
     */
    getInterfaces(inherited?: boolean): InterfaceType[];
    isClassType(): this is ClassType;
    private _getProperties;
    private _getMethods;
}
//# sourceMappingURL=class.d.ts.map