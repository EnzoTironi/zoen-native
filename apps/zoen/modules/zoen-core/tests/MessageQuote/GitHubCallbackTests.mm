#import <Foundation/Foundation.h>
#import "ENRMQuoteRanges.h"
#include "MD4CParser.hpp"
#include <cassert>
#include <memory>
#include <string>
#import "MarkdownASTNode.h"
#import "RenderedMarkdownSegment.h"
#import "github-bridge.inc"
#import "github-segments.inc"
@interface RenderContext : NSObject
- (NSDictionary *)getTextAttributes;
@end
@implementation RenderContext
- (NSDictionary *)getTextAttributes { return @{}; }
@end
@interface TextRenderer : NSObject
@end
@implementation TextRenderer
#import "github-text-render.inc"
@end

struct EnrichedMarkdownEventEmitter {
  struct Event { std::string itemText, selectedText; int selectionStart, selectionEnd; std::string sourceRangesJSON; };
  mutable Event last;
  void onContextMenuItemPress(Event event) const { last = event; }
};
static NSString *quoteTitle = @"Citar";
@interface FixtureSegmentText : NSObject
@property NSAttributedString *attributedText;
@end
@implementation FixtureSegmentText
@end
static NSAttributedString *ENRMGetAttributedText(FixtureSegmentText *text) { return text.attributedText; }
typedef void (^ENRMContextMenuPressHandler)(NSString *, NSString *, NSUInteger, NSUInteger);
@interface EnrichedMarkdown : NSObject {
@public std::shared_ptr<EnrichedMarkdownEventEmitter const> _eventEmitter;
}
- (void)runSegment:(FixtureSegmentText *)textView range:(NSRange)range;
@end
@implementation EnrichedMarkdown
#import "github-emitter.inc"
- (void)runSegment:(FixtureSegmentText *)textView range:(NSRange)range {
  __weak EnrichedMarkdown *weakSelf = self;
  ENRMContextMenuPressHandler handler =
#import "github-callback.inc"
  ;
  handler(quoteTitle, [textView.attributedText.string substringWithRange:range], range.location, NSMaxRange(range));
}
@end
static void append(MarkdownASTNode *node, NSMutableAttributedString *text) {
  if (node.type == MarkdownNodeTypeText) [[TextRenderer new] renderNodeContent:node into:text context:[RenderContext new]];
  for (MarkdownASTNode *child in node.children) append(child, text);
}
int main(int argc, const char *argv[]) { @autoreleasepool {
  if (argc > 1) quoteTitle = @(argv[1]);
  NSString *source = @"😀 same\n\n```txt\nskip\n```\n\n😀 **same**";
  Markdown::MD4CParser parser; Markdown::Md4cFlags flags;
  flags.noMedia = true; flags.latexMath = false;
  auto root = parser.parse(source.UTF8String, flags, true);
  // Execute the real Objective-C bridge, native GitHub segmenter and text-renderer
  // method. Only visual styling/point geometry is outside this Foundation fixture.
  MarkdownASTNode *ast = convertCppASTToObjC(root);
  NSArray *segments = ENRMSplitASTIntoSegments(ast);
  assert(segments.count == 3 && [segments[1] isKindOfClass:ENRMCodeBlockSegment.class]);
  ENRMTextSegment *second = segments[2];
  assert([second isKindOfClass:ENRMTextSegment.class] && second.nodes.firstObject == ast.children.lastObject);
  NSMutableAttributedString *visible = [[NSMutableAttributedString alloc] initWithString:@""];
  for (MarkdownASTNode *node in second.nodes) append(node, visible);
  assert([visible.string isEqualToString:@"😀 same"]);
  FixtureSegmentText *text = [FixtureSegmentText new]; text.attributedText = visible;
  EnrichedMarkdown *renderer = [EnrichedMarkdown new];
  renderer->_eventEmitter = std::make_shared<EnrichedMarkdownEventEmitter>();
  [renderer runSegment:text range:NSMakeRange(3, 4)];
  auto event = renderer->_eventEmitter->last;
  NSString *ranges = @(event.sourceRangesJSON.c_str());
  NSArray *spans = [NSJSONSerialization JSONObjectWithData:[ranges dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];
  NSRange expected = [source rangeOfString:@"same" options:NSBackwardsSearch];
  assert(spans.count == 1 && [spans[0][@"start_utf16"] unsignedIntegerValue] == expected.location);
  assert([spans[0][@"end_utf16"] unsignedIntegerValue] == NSMaxRange(expected));
  NSDictionary *payload = @{@"itemText": @(event.itemText.c_str()), @"selectedText": @(event.selectedText.c_str()),
    @"selectionStart": @(event.selectionStart), @"selectionEnd": @(event.selectionEnd), @"sourceRangesJSON": ranges};
  NSData *json = [NSJSONSerialization dataWithJSONObject:payload options:0 error:nil];
  puts([[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding].UTF8String);
} }
