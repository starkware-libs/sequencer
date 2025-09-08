import { TypeSystem } from './type-system';
export declare class Dependency {
    readonly system: TypeSystem;
    private readonly name;
    readonly version: string;
    constructor(system: TypeSystem, name: string, version: string);
    get assembly(): import("./assembly").Assembly;
}
//# sourceMappingURL=dependency.d.ts.map