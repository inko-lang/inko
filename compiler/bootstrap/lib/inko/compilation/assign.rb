module Inko
  module Compilation
    class Assign < Let
      def identifier(val_idx)
        name = variable_name

        if @code.local_defined?(name)
          depth, name_idx = @code.resolve_local(name)

          if depth
            @code.set_parent_local([name_idx, depth, val_idx], line, column)
          else
            @code.set_local([name_idx, val_idx], line, column)
          end

          val_idx
        else
          raise "Cannot re-assign undefined local variable #{name.inspect}"
        end
      end

      def variable_ast
        @ast.children[0]
      end

      def value_ast
        @ast.children[1]
      end

      def variable_name
        variable_ast.children[1]
      end
    end
  end
end
