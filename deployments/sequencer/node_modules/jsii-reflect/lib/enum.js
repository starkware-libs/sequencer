"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.EnumMember = exports.EnumType = void 0;
const docs_1 = require("./docs");
const type_1 = require("./type");
class EnumType extends type_1.Type {
    constructor(system, assembly, spec) {
        super(system, assembly, spec);
        this.system = system;
        this.assembly = assembly;
        this.spec = spec;
    }
    get members() {
        return this.spec.members.map((m) => new EnumMember(this, m));
    }
    isEnumType() {
        return true;
    }
}
exports.EnumType = EnumType;
class EnumMember {
    constructor(enumType, memberSpec) {
        this.enumType = enumType;
        this.name = memberSpec.name;
        this.docs = new docs_1.Docs(this.system, this, memberSpec.docs ?? {}, this.enumType.docs);
    }
    get system() {
        return this.enumType.system;
    }
    get assembly() {
        return this.enumType.assembly;
    }
}
exports.EnumMember = EnumMember;
//# sourceMappingURL=enum.js.map