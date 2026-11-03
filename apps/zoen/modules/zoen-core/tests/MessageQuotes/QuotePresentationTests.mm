#import <AppKit/AppKit.h>
#import "MarkdownASTNode.h"
#import "MarkdownASTSerializer.h"
#import "ENRMCodeBlockContent.h"
#import "ENRMQuotePresentation.h"
#import "RenderedMarkdownSegment.h"
#include "MD4CParser.hpp"
#include "quote-bridge.inc"
#include "quote-code-content.inc"

static MarkdownASTNode *parse(NSString *source) {
  Markdown::MD4CParser parser;
  Markdown::Md4cFlags flags;
  flags.hardSoftBreaks = true; flags.preserveBlankLines = true;
  flags.latexMath = false; flags.noMedia = true;
  return convertCppASTToObjC(parser.parse(source.UTF8String, flags, true));
}
static NSAttributedString *text(NSString *value, CGFloat size, CGFloat lineHeight) {
  NSMutableParagraphStyle *paragraph = [NSMutableParagraphStyle new];
  paragraph.minimumLineHeight = lineHeight; paragraph.maximumLineHeight = lineHeight;
  return [[NSAttributedString alloc] initWithString:value attributes:@{
    NSFontAttributeName:[NSFont systemFontOfSize:size], NSParagraphStyleAttributeName:paragraph
  }];
}
int main() { @autoreleasepool {
  NSString *source = @"> **forte** _café_ `x\\y` [🦊](https://example.org/a_(b))\n> segunda\n> terceira\n> quarta\n\nbody\n\n> other";
  MarkdownASTNode *root = parse(source);
  NSString *originalCopy = markdownFromASTNode(root);
  NSString *originalPlain = plainTextFromASTNode(root);
  ENRMConfigureQuoteNodes(root, YES, [NSSet set]);
  uint64_t collapsedSignature = ENRMSignatureForNode(root);
  MarkdownASTNode *first = root.children.firstObject;
  NSCAssert(first.type == MarkdownNodeTypeBlockquote, @"actual parser quote");
  NSCAssert([first.attributes[@"enrmQuoteIndex"] isEqual:@"0"], @"first stable AST block identity");
  NSCAssert(![first.attributes[@"enrmQuoteExpanded"] boolValue], @"starts collapsed without animation");
  MarkdownASTNode *last = root.children.lastObject;
  NSCAssert([last.attributes[@"enrmQuoteIndex"] isEqual:@"1"], @"independent second quote");
  ENRMConfigureQuoteNodes(root, YES, [NSSet setWithObject:@0]);
  NSCAssert(ENRMSignatureForNode(root) != collapsedSignature, @"expansion updates the actual native segment signature");
  NSCAssert([first.attributes[@"enrmQuoteExpanded"] boolValue], @"only selected quote expands");
  NSCAssert(![last.attributes[@"enrmQuoteExpanded"] boolValue], @"other quote unchanged");
  NSCAssert([markdownFromASTNode(root) isEqual:originalCopy], @"copy keeps full original renderer source");
  NSCAssert([plainTextFromASTNode(root) isEqual:originalPlain], @"selection keeps all original text and Unicode");
  ENRMConfigureQuoteNodes(root, YES, [NSSet set]);
  NSCAssert(ENRMSignatureForNode(root) == collapsedSignature, @"fold signature is deterministic without source changes");
  ENRMConfigureQuoteNodes(root, NO, [NSSet setWithObject:@0]);
  NSCAssert(first.attributes[@"enrmQuoteIndex"] == nil, @"selection/document renderer fully expanded");
  for (NSString *complex in @[@"> outer\n>> nested", @"> ```md\n> **code**\n> ```", @"> | a | b |\n> | - | - |\n> | c | d |"] ) {
    MarkdownASTNode *mixed = parse(complex);
    NSString *copy = markdownFromASTNode(mixed);
    ENRMConfigureQuoteNodes(mixed, YES, [NSSet set]);
    NSCAssert(mixed.children.firstObject.attributes[@"enrmQuoteIndex"] == nil, @"mixed GFM stays fully rendered");
    NSCAssert([markdownFromASTNode(mixed) isEqual:copy], @"complex source remains exact");
  }
  for (NSString *literal in @[@"literal > comparison", @"\\> escaped prefix", @"```md\n> inside code\n```"] ) {
    MarkdownASTNode *ordinary = parse(literal);
    ENRMConfigureQuoteNodes(ordinary, YES, [NSSet set]);
    NSCAssert(ordinary.children.firstObject.attributes[@"enrmQuoteIndex"] == nil, @"the real AST, not a marker regexp, owns eligibility");
  }
  NSCAssert(ENRMQuotePreviewHeight(text(@"one\ntwo\nthree",16,24),240,3) == 0, @"three lines need no control");
  CGFloat preview = ENRMQuotePreviewHeight(text(@"one\ntwo\nthree\nfour",16,24),240,3);
  NSCAssert(preview == 72, @"actual TextKit first three line fragments");
  NSCAssert(ENRMQuotePreviewHeight(text(@"one\ntwo\nthree\nfour",32,48),240,3) == 144, @"Dynamic Type grows real preview");
  NSAttributedString *wrap = text(@"café 🦊 words words words words words words words words words words",16,24);
  NSCAssert(ENRMQuotePreviewHeight(wrap,70,3) > 0, @"narrow native layout wraps into collapsible lines");
  NSCAssert(ENRMQuotePreviewHeight(wrap,2000,3) == 0, @"rotation wider layout needs no control");
  NSCAssert(ENRMQuotePreviewHeight(text(@"",16,24),240,3) == 0, @"empty body");
  NSCAssert(ENRMQuoteRestoreOffset(400,700,500,-62,1000) == 600, @"expansion retains tapped reading anchor");
  NSCAssert(ENRMQuoteRestoreOffset(600,300,500,-62,1000) == 400, @"collapse restores anchor");
  NSCAssert(ENRMQuoteRestoreOffset(0,100,500,-62,1000) == -62, @"top boundary");
  NSCAssert(ENRMQuoteRestoreOffset(900,900,500,-62,1000) == 1000, @"bottom and keyboard boundary");
  NSCAssert(ENRMQuoteGeometryProgress(150,100,200) == 0.5, @"committed half geometry");
  CGRect half = ENRMQuoteGeometryFrame(CGRectMake(0,20,200,100),CGRectMake(0,120,200,300),0.5);
  NSCAssert(half.origin.y == 70 && half.size.height == 200, @"viewport and sibling position share progress");
  CGRect interrupted = ENRMQuoteGeometryFrame(half,CGRectMake(0,20,200,100),0);
  NSCAssert(CGRectEqualToRect(half, interrupted), @"interruption begins from current presentation");
  puts("Quote AST, copy, TextKit preview and reading anchor PASS");
} }
