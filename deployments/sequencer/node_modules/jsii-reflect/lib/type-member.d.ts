import { Documentable } from './docs';
import { Initializer } from './initializer';
import { Method } from './method';
import { Property } from './property';
import { SourceLocatable } from './source';
export interface TypeMember extends Documentable, SourceLocatable {
    name: string;
    abstract: boolean;
    kind: MemberKind;
    protected?: boolean;
}
export declare enum MemberKind {
    Initializer = "initializer",
    Method = "method",
    Property = "property"
}
export declare function isInitializer(x: TypeMember): x is Initializer;
export declare function isMethod(x: TypeMember): x is Method;
export declare function isProperty(x: TypeMember): x is Property;
//# sourceMappingURL=type-member.d.ts.map