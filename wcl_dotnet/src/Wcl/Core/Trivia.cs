using System.Collections.Generic;

namespace Wcl.Core
{
    public enum CommentStyle
    {
        Line,
        Block,
        Doc
    }

    public enum CommentPlacement
    {
        Leading,
        Trailing
    }

    public class Comment
    {
        public string Text { get; }
        public CommentStyle Style { get; }
        public CommentPlacement Placement { get; }

        public Comment(string text, CommentStyle style, CommentPlacement placement)
        {
            Text = text;
            Style = style;
            Placement = placement;
        }
    }

    public class Trivia
    {
        public List<Comment> Comments { get; }
        public int LeadingNewlines { get; }

        public Trivia(List<Comment>? comments = null, int leadingNewlines = 0)
        {
            Comments = comments ?? new List<Comment>();
            LeadingNewlines = leadingNewlines;
        }

        public static Trivia Empty() => new Trivia();
    }
}
