using System.Collections.Generic;
using System.Linq;
using Wcl.Core;

namespace Wcl.Eval
{
    public class BlockRef
    {
        public string Kind { get; set; }
        public string? Id { get; set; }
        public List<string> Labels { get; set; }
        public OrderedMap<string, WclValue> Attributes { get; set; }
        public List<BlockRef> Children { get; set; }
        public List<DecoratorValue> Decorators { get; set; }
        public Span Span { get; set; }

        public BlockRef(string kind, string? id, List<string> labels,
                        OrderedMap<string, WclValue> attributes, List<BlockRef> children,
                        List<DecoratorValue> decorators, Span span)
        {
            Kind = kind; Id = id; Labels = labels;
            Attributes = attributes; Children = children;
            Decorators = decorators; Span = span;
        }

        public bool HasDecorator(string name) => Decorators.Any(d => d.Name == name);

        public DecoratorValue? GetDecorator(string name) =>
            Decorators.FirstOrDefault(d => d.Name == name);

        public WclValue? Get(string key) =>
            Attributes.TryGetValue(key, out var val) ? val : null;
    }

    public class DecoratorValue
    {
        public string Name { get; set; }
        public OrderedMap<string, WclValue> Args { get; set; }

        public DecoratorValue(string name, OrderedMap<string, WclValue> args)
        {
            Name = name;
            Args = args;
        }
    }

    public class FunctionValue
    {
        public List<string> Params { get; set; }
        public FunctionBody Body { get; set; }
        public ScopeId? ClosureScope { get; set; }

        public FunctionValue(List<string> parms, FunctionBody body, ScopeId? closureScope = null)
        {
            Params = parms; Body = body; ClosureScope = closureScope;
        }
    }

    public abstract class FunctionBody { }

    public sealed class BuiltinFunctionBody : FunctionBody
    {
        public string Name { get; }
        public BuiltinFunctionBody(string name) => Name = name;
    }

    public sealed class UserDefinedFunctionBody : FunctionBody
    {
        public Core.Ast.Expr Expr { get; }
        public UserDefinedFunctionBody(Core.Ast.Expr expr) => Expr = expr;
    }

    public sealed class BlockExprFunctionBody : FunctionBody
    {
        public List<(string Name, Core.Ast.Expr Expr)> Lets { get; }
        public Core.Ast.Expr FinalExpr { get; }
        public BlockExprFunctionBody(List<(string, Core.Ast.Expr)> lets, Core.Ast.Expr finalExpr)
        { Lets = lets; FinalExpr = finalExpr; }
    }
}
