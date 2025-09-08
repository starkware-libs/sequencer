import * as spec from './assembly';
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
export declare class NameTree {
    static of(assm: spec.Assembly): NameTree;
    private _children;
    private _fqn?;
    private constructor();
    /** The children of this node, by name. */
    get children(): {
        readonly [name: string]: NameTree;
    };
    /** The fully qualified name of the type at this node, if there is one. */
    get fqn(): string | undefined;
    /**
     * Adds a type to this ``NameTree``.
     *
     * @param type the type to be added.
     * @param path the path at which to add the node under this tree.
     */
    private register;
}
//# sourceMappingURL=name-tree.d.ts.map