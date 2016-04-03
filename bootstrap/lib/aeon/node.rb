module Aeon
  class Node < AST::Node
    attr_reader :line, :column
  end
end
