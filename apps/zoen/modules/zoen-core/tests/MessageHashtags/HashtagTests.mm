#import <Foundation/Foundation.h>
#import "ENRMMarkdownParser.h"
#include <cassert>

static void inspect(MarkdownASTNode *node, NSMutableArray *links, NSMutableString *visible, NSString *source) {
  if (node.type == MarkdownNodeTypeLink) [links addObject:node.attributes[@"url"]];
  if (node.type == MarkdownNodeTypeText) {
    [visible appendString:node.content];
    NSString *start = node.attributes[@"quoteStartUtf16"], *end = node.attributes[@"quoteEndUtf16"];
    if (start && end) {
      assert([[source substringWithRange:NSMakeRange(start.integerValue, end.integerValue-start.integerValue)] isEqual:node.content]);
    }
  }
  for (MarkdownASTNode *child in node.children) inspect(child, links, visible, source);
}
static NSArray *parse(NSString *source, BOOL enabled, NSMutableString *visible) {
  ENRMMarkdownParser *parser = [ENRMMarkdownParser new];
  ENRMMd4cFlags *flags = [ENRMMd4cFlags defaultFlags]; flags.hashtags = enabled;
  NSMutableArray *links = [NSMutableArray new];
  inspect([parser parseMarkdown:source flags:flags], links, visible, source);
  return links;
}
int main() { @autoreleasepool {
  NSMutableString *visible = [NSMutableString new];
  NSArray *links = parse(@"😀 **#Ação_2026** and (#发布), #release!", YES, visible);
  assert([visible isEqual:@"😀 #Ação_2026 and (#发布), #release!"]);
  assert(links.count == 3);
  assert([links[0] isEqual:@"zoen-hashtag:%23A%C3%A7%C3%A3o_2026"]);
  assert([links[1] isEqual:@"zoen-hashtag:%23%E5%8F%91%E5%B8%83"]);
  assert([links[2] isEqual:@"zoen-hashtag:%23release"]);
  assert(parse(@"#release", NO, [NSMutableString new]).count == 0);
  assert(parse(@"word#release https://example.com/#release `#release`", YES, [NSMutableString new]).count == 1); // Existing network link only.
  assert(parse(@"\\#release\n```\n#release\n```\n#___", YES, [NSMutableString new]).count == 0);
  links = parse(@"[#label](https://example.com) #ok", YES, [NSMutableString new]);
  assert(links.count == 2 && [links[0] isEqual:@"https://example.com"]);
  puts("Native hashtag AST, literal text and source offsets PASS");
} }
