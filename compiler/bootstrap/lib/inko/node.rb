module Inko
  class Node < AST::Node
    attr_reader :line, :column
  end
end
