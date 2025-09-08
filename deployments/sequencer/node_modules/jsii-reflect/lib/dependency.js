"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Dependency = void 0;
class Dependency {
    constructor(system, name, version) {
        this.system = system;
        this.name = name;
        this.version = version;
    }
    get assembly() {
        return this.system.findAssembly(this.name);
    }
}
exports.Dependency = Dependency;
//# sourceMappingURL=dependency.js.map