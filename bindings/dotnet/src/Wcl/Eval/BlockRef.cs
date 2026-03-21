using System.Collections.Generic;
using System.Linq;
using Wcl.Core;

namespace Wcl.Eval
{
    public class BlockRef
    {
        public string Kind { get; set; }
        public string? Id { get; set; }
        public OrderedMap<string, WclValue> Attributes { get; set; }
        public List<BlockRef> Children { get; set; }
        public List<DecoratorValue> Decorators { get; set; }

        public BlockRef(string kind, string? id,
                        OrderedMap<string, WclValue> attributes, List<BlockRef> children,
                        List<DecoratorValue> decorators)
        {
            Kind = kind; Id = id;
            Attributes = attributes; Children = children;
            Decorators = decorators;
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
}
