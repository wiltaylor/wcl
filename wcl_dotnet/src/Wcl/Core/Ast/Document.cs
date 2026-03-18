using System.Collections.Generic;

namespace Wcl.Core.Ast
{
    public class Document
    {
        public List<DocItem> Items { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }

        public Document(List<DocItem> items, Trivia trivia, Span span)
        {
            Items = items;
            Trivia = trivia;
            Span = span;
        }
    }
}
