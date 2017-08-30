# frozen_string_literal: true

module Inkoc
  module Type
    class Float
      include Inspect
      include ObjectOperations
      include TypeCompatibility
      include ImmutableType

      def type_name
        'Float'
      end
    end
  end
end
