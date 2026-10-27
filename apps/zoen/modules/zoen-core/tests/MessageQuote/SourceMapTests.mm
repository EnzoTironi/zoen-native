#import <Foundation/Foundation.h>
#import "ENRMQuoteRanges.h"
#include "MD4CParser.hpp"
#include <cassert>

static void append(const std::shared_ptr<Markdown::MarkdownASTNode> &node, NSMutableAttributedString *text) {
  if (node->type == Markdown::NodeType::Text) {
    NSString *value = [[NSString alloc] initWithBytes:node->content.data() length:node->content.size() encoding:NSUTF8StringEncoding];
    NSMutableDictionary *attributes = [NSMutableDictionary new];
    auto start = node->attributes.find("quoteStartUtf16"), end = node->attributes.find("quoteEndUtf16");
    if (start != node->attributes.end() && end != node->attributes.end() &&
        std::stoul(end->second) - std::stoul(start->second) == value.length) {
      attributes[@"ENRMQuoteSource"] = @{@"start": @(std::stoul(start->second)), @"text": value};
    }
    [text appendAttributedString:[[NSAttributedString alloc] initWithString:value attributes:attributes]];
  }
  for (auto &child : node->children) append(child, text);
}
static NSArray *ranges(NSAttributedString *text, NSRange selected) {
  NSString *json = ENRMQuoteRanges(text, selected);
  return json.length ? [NSJSONSerialization JSONObjectWithData:[json dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil] : nil;
}
int main() { @autoreleasepool {
  Markdown::MD4CParser parser;
  Markdown::Md4cFlags flags; flags.noMedia = true; flags.latexMath = false; flags.hardSoftBreaks = true;
  NSString *source = @"😀 same **same** [ação](https://example.invalid) `x\\*y`";
  auto ast = parser.parse(source.UTF8String, flags, true);
  NSMutableAttributedString *visible = [[NSMutableAttributedString alloc] initWithString:@""];
  append(ast, visible);
  assert([visible.string hasPrefix:@"😀 same same ação"]);
  // Actual MD4C anchors distinguish the second identical word, after an astral scalar.
  NSArray *second = ranges(visible, NSMakeRange(8, 4));
  assert(second.count == 1);
  assert([second[0][@"start_utf16"] unsignedIntegerValue] == 10);
  assert([second[0][@"end_utf16"] unsignedIntegerValue] == 14);
  NSArray *multiple = ranges(visible, NSMakeRange(8, 9));
  NSMutableString *restored = [NSMutableString new];
  for (NSDictionary *span in multiple) {
    NSUInteger start = [span[@"start_utf16"] unsignedIntegerValue], end = [span[@"end_utf16"] unsignedIntegerValue];
    [restored appendString:[source substringWithRange:NSMakeRange(start, end-start)]];
  }
  assert([restored isEqualToString:@"same ação"]);
  assert(!ranges(visible, NSMakeRange(0, 0)));
  assert(!ranges(visible, NSMakeRange(999, 1)));
  // A selected synthetic layout character poisons the WHOLE selection, never a prefix.
  [visible insertAttributedString:[[NSAttributedString alloc] initWithString:@"\n"] atIndex:10];
  assert(!ranges(visible, NSMakeRange(8, 6)));
  auto escaped = parser.parse("a\\*b", flags, true);
  NSMutableAttributedString *escapedText = [[NSMutableAttributedString alloc] initWithString:@""];
  append(escaped, escapedText);
  assert([escapedText.string isEqualToString:@"a*b"]);
  NSArray *escapedSpans = ranges(escapedText, NSMakeRange(0, 3));
  assert(escapedSpans.count > 1);
  NSString *escapedSource = @"a\\*b";
  NSMutableString *escapedRestored = [NSMutableString new];
  for (NSDictionary *span in escapedSpans) {
    NSUInteger start = [span[@"start_utf16"] unsignedIntegerValue], end = [span[@"end_utf16"] unsignedIntegerValue];
    [escapedRestored appendString:[escapedSource substringWithRange:NSMakeRange(start, end-start)]];
  }
  assert([escapedRestored isEqualToString:@"a*b"]);
  puts("Native quote source map PASS");
} }
