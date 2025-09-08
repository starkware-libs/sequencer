"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Enum = void 0;
const runtime_1 = require("../runtime");
const go_type_1 = require("./go-type");
class Enum extends go_type_1.GoType {
    constructor(pkg, type) {
        super(pkg, type);
        this.members = type.members.map((mem) => new GoEnumMember(this, mem));
    }
    get parameterValidators() {
        return [];
    }
    emit(context) {
        this.emitDocs(context);
        const { code } = context;
        // TODO figure out the value type -- probably a string in most cases
        const valueType = 'string';
        code.line(`type ${this.name} ${valueType}`);
        code.line();
        code.open(`const (`);
        // Const values are prefixed by the wrapped value type
        for (const member of this.members) {
            member.emit(context);
        }
        code.close(`)`);
        code.line();
    }
    emitRegistration({ code }) {
        code.open(`${runtime_1.JSII_RT_ALIAS}.RegisterEnum(`);
        code.line(`"${this.fqn}",`);
        code.line(`reflect.TypeOf((*${this.name})(nil)).Elem(),`);
        code.open(`map[string]interface{}{`);
        for (const member of this.members) {
            code.line(`"${member.rawValue}": ${member.name},`);
        }
        code.close(`},`);
        code.close(')');
    }
    get dependencies() {
        return [];
    }
    get specialDependencies() {
        return {
            fmt: false,
            init: false,
            internal: false,
            runtime: false,
            time: false,
        };
    }
}
exports.Enum = Enum;
class GoEnumMember {
    constructor(parent, entry) {
        this.parent = parent;
        this.name = `${parent.name}_${entry.name}`;
        this.rawValue = entry.name;
        this.docs = entry.docs;
        this.apiLocation = {
            api: 'member',
            fqn: this.parent.fqn,
            memberName: entry.name,
        };
    }
    emit({ code, documenter }) {
        documenter.emit(this.docs, this.apiLocation);
        code.line(`${this.name} ${this.parent.name} = "${this.rawValue}"`);
    }
}
//# sourceMappingURL=enum.js.map