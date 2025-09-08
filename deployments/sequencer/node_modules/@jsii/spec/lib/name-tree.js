"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.NameTree = void 0;
/**
 * A tree of all names in a module. A node represent a type (terminal)
 * and may represent another node in the namespace (at the same time).
 * Therefore, a key of '_' represents a terminal and references the fqn
 * of the type.
 *
 * For example, say we have the following types:
 *   - aws.ec2.Host
 *   - aws.ec2.Instance
 *   - aws.ec2.Instance.Subtype
 *
 * the the name tree will look like this:
 *
 * module: {
 *   children: {
 *     aws: {
 *       children {
 *         ec2: {
 *           children: {
 *             Host: {
 *               fqn: 'aws.ec2.Host',
 *               children: {}
 *             },
 *             Instance: {
 *               fqn: 'aws.ec2.Host',
 *               children: {
 *                 Subtype: {
 *                   fqn: 'aws.ec2.Host.Subtype',
 *                   children: {}
 *                 }
 *               }
 *             }
 *           }
 *         }
 *       }
 *     }
 *   }
 * }
 */
class NameTree {
    static of(assm) {
        const nameTree = new NameTree();
        for (const type of Object.values(assm.types ?? {})) {
            nameTree.register(type.fqn);
        }
        return nameTree;
    }
    /* NameTree.of(assembly) should be used. */
    constructor() {
        this._children = {};
    }
    /** The children of this node, by name. */
    get children() {
        return this._children;
    }
    /** The fully qualified name of the type at this node, if there is one. */
    get fqn() {
        return this._fqn;
    }
    /**
     * Adds a type to this ``NameTree``.
     *
     * @param type the type to be added.
     * @param path the path at which to add the node under this tree.
     */
    register(fqn, path = fqn.split('.')) {
        if (path.length === 0) {
            this._fqn = fqn;
        }
        else {
            const [head, ...rest] = path;
            if (!this._children[head]) {
                this._children[head] = new NameTree();
            }
            this._children[head].register(fqn, rest);
        }
        return this;
    }
}
exports.NameTree = NameTree;
//# sourceMappingURL=name-tree.js.map