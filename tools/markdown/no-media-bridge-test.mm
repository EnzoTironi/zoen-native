#import "ENRMMarkdownParser.h"
#include <iostream>
static NSUInteger countNodes(MarkdownASTNode *node, MarkdownNodeType type) {
  NSUInteger count = node.type == type;
  for (MarkdownASTNode *child in node.children) count += countNodes(child,type);
  return count;
}
int main() {
  @autoreleasepool {
    ENRMMd4cFlags *flags = [ENRMMd4cFlags defaultFlags];
    if (flags.noMedia || !flags.latexMath || !flags.admonitions) return 1;
    flags.noMedia = YES;
    flags.latexMath = NO;
    ENRMMd4cFlags *copy = [flags copy];
    if (!copy.noMedia || copy.latexMath || !copy.admonitions) return 2;
    ENRMMarkdownParser *parser = [ENRMMarkdownParser new];
    NSString *source = @"# Real bridge\n\n![explicit alt](https://example.invalid/image.png)\n\n<video src=\"https://example.invalid/video.mp4\"></video>\n\n```cpp\nx < y; // ![literal](file:///not-read)\n```";
    MarkdownASTNode *safe = [parser parseMarkdown:source flags:copy];
    if (countNodes(safe,MarkdownNodeTypeImage) || countNodes(safe,MarkdownNodeTypeVideo)) return 3;
    if (!countNodes(safe,MarkdownNodeTypeHeading) || !countNodes(safe,MarkdownNodeTypeCodeBlock)) return 4;
    MarkdownASTNode *normal = [parser parseMarkdown:@"![normal](https://example.invalid/image.png)"];
    if (countNodes(normal,MarkdownNodeTypeImage) != 1) return 5;
    copy.noMedia = NO;
    if ([flags noMedia] != YES) return 6;
    std::cout << "PASS actual Objective-C flags init/copy and Objective-C→C++→Objective-C parser bridge; default images retained\n";
  }
  return 0;
}
