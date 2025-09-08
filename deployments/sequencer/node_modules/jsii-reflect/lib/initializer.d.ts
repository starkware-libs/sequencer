import { Callable } from './callable';
import { Documentable } from './docs';
import { Overridable } from './overridable';
import { SourceLocatable } from './source';
import { MemberKind, TypeMember } from './type-member';
export declare class Initializer extends Callable implements Documentable, Overridable, TypeMember, SourceLocatable {
    static isInitializer(x: Callable): x is Initializer;
    readonly kind = MemberKind.Initializer;
    readonly name = "<initializer>";
    readonly abstract = false;
}
//# sourceMappingURL=initializer.d.ts.map