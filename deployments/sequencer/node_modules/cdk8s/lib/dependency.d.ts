import { Node, IConstruct } from 'constructs';
/**
 * Represents the dependency graph for a given Node.
 *
 * This graph includes the dependency relationships between all nodes in the
 * node (construct) sub-tree who's root is this Node.
 *
 * Note that this means that lonely nodes (no dependencies and no dependants) are also included in this graph as
 * childless children of the root node of the graph.
 *
 * The graph does not include cross-scope dependencies. That is, if a child on the current scope depends on a node
 * from a different scope, that relationship is not represented in this graph.
 *
 */
export declare class DependencyGraph {
    private readonly _fosterParent;
    constructor(node: Node);
    /**
     * Returns the root of the graph.
     *
     * Note that this vertex will always have `null` as its `.value` since it is an artifical root
     * that binds all the connected spaces of the graph.
     */
    get root(): DependencyVertex;
    /**
     * @see Vertex.topology()
     */
    topology(): IConstruct[];
}
/**
 * Represents a vertex in the graph.
 *
 * The value of each vertex is an `IConstruct` that is accessible via the `.value` getter.
 */
export declare class DependencyVertex {
    private readonly _value;
    private readonly _children;
    private readonly _parents;
    constructor(value?: IConstruct | undefined);
    /**
     * Returns the IConstruct this graph vertex represents.
     *
     * `null` in case this is the root of the graph.
     */
    get value(): IConstruct | undefined;
    /**
     * Returns the children of the vertex (i.e dependencies)
     */
    get outbound(): Array<DependencyVertex>;
    /**
     * Returns the parents of the vertex (i.e dependants)
     */
    get inbound(): Array<DependencyVertex>;
    /**
     * Returns a topologically sorted array of the constructs in the sub-graph.
     */
    topology(): IConstruct[];
    /**
     * Adds a vertex as a dependency of the current node.
     * Also updates the parents of `dep`, so that it contains this node as a parent.
     *
     * This operation will fail in case it creates a cycle in the graph.
     *
     * @param dep The dependency
     */
    addChild(dep: DependencyVertex): void;
    private addParent;
    private findRoute;
}
