"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.TypeSystemTree = void 0;
const spec_1 = require("@jsii/spec");
// eslint-disable-next-line @typescript-eslint/no-require-imports
const chalk = require("chalk");
const oo_ascii_tree_1 = require("oo-ascii-tree");
const method_1 = require("./method");
const optional_value_1 = require("./optional-value");
const property_1 = require("./property");
/**
 * Visualizes a `TypeSystem` as an ASCII tree.
 */
class TypeSystemTree extends oo_ascii_tree_1.AsciiTree {
    constructor(typesys, options = {}) {
        super();
        if (options.showAll) {
            options.dependencies = true;
            options.inheritance = true;
            options.members = true;
            options.signatures = true;
            options.types = true;
        }
        const shouldUseColors = options.colors ?? true;
        withColors(shouldUseColors, () => {
            if (typesys.assemblies.length > 0) {
                const assemblies = new TitleNode('assemblies');
                assemblies.add(...typesys.assemblies.map((a) => new AssemblyNode(a, options)));
                this.add(assemblies);
            }
        });
    }
}
exports.TypeSystemTree = TypeSystemTree;
class AssemblyNode extends oo_ascii_tree_1.AsciiTree {
    constructor(assembly, options) {
        super(chalk.green(assembly.name));
        if (options.dependencies && assembly.dependencies.length > 0) {
            const deps = new TitleNode('dependencies');
            this.add(deps);
            deps.add(...assembly.dependencies.map((d) => new DependencyNode(d, options)));
        }
        const submodules = assembly.submodules;
        if (submodules.length > 0) {
            const title = new TitleNode('submodules');
            this.add(title);
            title.add(...submodules.map((s) => new SubmoduleNode(s, options)));
        }
        if (options.types) {
            const types = new TitleNode('types');
            this.add(types);
            types.add(...assembly.classes.map((c) => new ClassNode(c, options)));
            types.add(...assembly.interfaces.map((c) => new InterfaceNode(c, options)));
            types.add(...assembly.enums.map((c) => new EnumNode(c, options)));
        }
    }
}
class SubmoduleNode extends oo_ascii_tree_1.AsciiTree {
    constructor(submodule, options) {
        super(chalk.green(submodule.name));
        const submodules = submodule.submodules;
        if (submodules.length > 0) {
            const title = new TitleNode('submodules');
            this.add(title);
            title.add(...submodules.map((s) => new SubmoduleNode(s, options)));
        }
        if (options.types) {
            const types = new TitleNode('types');
            this.add(types);
            types.add(...submodule.classes.map((c) => new ClassNode(c, options)));
            types.add(...submodule.interfaces.map((i) => new InterfaceNode(i, options)));
            types.add(...submodule.enums.map((e) => new EnumNode(e, options)));
        }
    }
}
class MethodNode extends oo_ascii_tree_1.AsciiTree {
    constructor(method, options) {
        const args = method.parameters.map((p) => p.name).join(',');
        super(`${maybeStatic(method)}${method.name}(${args}) ${chalk.gray('method')}${describeStability(method, options)}`);
        if (options.signatures) {
            if (method.abstract) {
                this.add(new FlagNode('abstract'));
            }
            if (method.protected) {
                this.add(new FlagNode('protected'));
            }
            if (method.static) {
                this.add(new FlagNode('static'));
            }
            if (method.variadic) {
                this.add(new FlagNode('variadic'));
            }
            if (method.parameters.length > 0) {
                const params = new TitleNode('parameters');
                this.add(params);
                params.add(...method.parameters.map((p) => new ParameterNode(p, options)));
            }
            this.add(new OptionalValueNode('returns', method.returns, {
                asPromise: method.async,
            }));
        }
    }
}
class InitializerNode extends oo_ascii_tree_1.AsciiTree {
    constructor(initializer, options) {
        const args = initializer.parameters.map((p) => p.name).join(',');
        super(`${initializer.name}(${args}) ${chalk.gray('initializer')}${describeStability(initializer, options)}`);
        if (options.signatures) {
            if (initializer.protected) {
                this.add(new FlagNode('protected'));
            }
            if (initializer.variadic) {
                this.add(new FlagNode('variadic'));
            }
            if (initializer.parameters.length > 0) {
                const params = new TitleNode('parameters');
                this.add(params);
                params.add(...initializer.parameters.map((p) => new ParameterNode(p, options)));
            }
        }
    }
}
class ParameterNode extends oo_ascii_tree_1.AsciiTree {
    constructor(param, _options) {
        super(param.name);
        this.add(new OptionalValueNode('type', param));
        if (param.variadic) {
            this.add(new FlagNode('variadic'));
        }
    }
}
class PropertyNode extends oo_ascii_tree_1.AsciiTree {
    constructor(property, options) {
        super(`${maybeStatic(property)}${property.name} ${chalk.gray('property')}${describeStability(property, options)}`);
        if (options.signatures) {
            if (property.abstract) {
                this.add(new FlagNode('abstract'));
            }
            if (property.const) {
                this.add(new FlagNode('const'));
            }
            if (property.immutable) {
                this.add(new FlagNode('immutable'));
            }
            if (property.protected) {
                this.add(new FlagNode('protected'));
            }
            if (property.static) {
                this.add(new FlagNode('static'));
            }
            this.add(new OptionalValueNode('type', property));
        }
    }
}
class OptionalValueNode extends oo_ascii_tree_1.AsciiTree {
    constructor(name, optionalValue, { asPromise } = { asPromise: false }) {
        let type = optional_value_1.OptionalValue.describe(optionalValue);
        if (asPromise) {
            type = `Promise<${type}>`;
        }
        super(`${chalk.underline(name)}: ${type}`);
    }
}
class ClassNode extends oo_ascii_tree_1.AsciiTree {
    constructor(type, options) {
        super(`${chalk.gray('class')} ${chalk.cyan(type.name)}${describeStability(type, options)}`);
        if (options.inheritance && type.base) {
            this.add(new KeyValueNode('base', type.base.name));
        }
        if (options.inheritance && type.interfaces.length > 0) {
            this.add(new KeyValueNode('interfaces', type.interfaces.map((i) => i.name).join(',')));
        }
        if (options.members) {
            const members = new TitleNode('members');
            this.add(members);
            if (type.initializer) {
                members.add(new InitializerNode(type.initializer, options));
            }
            members.add(...type.ownMethods.map((m) => new MethodNode(m, options)));
            members.add(...type.ownProperties.map((p) => new PropertyNode(p, options)));
        }
    }
}
class InterfaceNode extends oo_ascii_tree_1.AsciiTree {
    constructor(type, options) {
        super(`${chalk.gray('interface')} ${chalk.cyan(type.name)}${describeStability(type, options)}`);
        if (options.inheritance && type.interfaces.length > 0) {
            const interfaces = new TitleNode('interfaces');
            this.add(interfaces);
            interfaces.add(...type.interfaces.map((i) => new TextNode(i.name)));
        }
        if (options.members) {
            const members = new TitleNode('members');
            members.add(...type.ownMethods.map((m) => new MethodNode(m, options)));
            members.add(...type.ownProperties.map((p) => new PropertyNode(p, options)));
            this.add(members);
        }
    }
}
class EnumNode extends oo_ascii_tree_1.AsciiTree {
    constructor(enumType, options) {
        super(`${chalk.gray('enum')} ${chalk.cyan(enumType.name)}${describeStability(enumType, options)}`);
        if (options.members) {
            enumType.members.forEach((mem) => {
                this.add(new oo_ascii_tree_1.AsciiTree(mem.name + describeStability(mem, options)));
            });
        }
    }
}
class DependencyNode extends oo_ascii_tree_1.AsciiTree {
    constructor(dep, _options) {
        super(dep.assembly.name);
    }
}
class TitleNode extends oo_ascii_tree_1.AsciiTree {
    constructor(name, children = []) {
        super(chalk.underline(name), ...children);
    }
}
class KeyValueNode extends oo_ascii_tree_1.AsciiTree {
    constructor(key, value) {
        super(`${chalk.underline(key)}: ${value}`);
    }
}
class TextNode extends oo_ascii_tree_1.AsciiTree {
}
class FlagNode extends oo_ascii_tree_1.AsciiTree {
    constructor(flag) {
        super(chalk.italic(flag));
    }
}
/**
 * Invokes `block` with colors enabled/disabled and reverts to old value afterwards.
 */
function withColors(enabled, block) {
    const oldLevel = chalk.level;
    try {
        if (!enabled) {
            chalk.level = 0; // No colors at all
        }
        block();
    }
    finally {
        chalk.level = oldLevel;
    }
}
function describeStability(thing, options) {
    if (!options.stabilities || thing.docs.stability == null) {
        return '';
    }
    switch (thing.docs.stability) {
        case spec_1.Stability.Stable:
            return ` (${chalk.green('stable')})`;
        case spec_1.Stability.External:
            return ` (${chalk.green('external')})`;
        case spec_1.Stability.Experimental:
            return ` (${chalk.yellow('experimental')})`;
        case spec_1.Stability.Deprecated:
            return ` (${chalk.red('deprecated')})`;
        default:
            return '';
    }
}
function maybeStatic(mem) {
    let isStatic;
    if (mem instanceof property_1.Property) {
        isStatic = !!mem.static;
    }
    if (mem instanceof method_1.Method) {
        isStatic = !!mem.static;
    }
    return isStatic ? `${chalk.grey('static')} ` : '';
}
//# sourceMappingURL=tree.js.map