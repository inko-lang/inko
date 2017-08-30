# frozen_string_literal: true

module Inkoc
  module Type
    class Integer
      include Inspect
      include ObjectOperations
      include TypeCompatibility
      include ImmutableType

      def type_name
        'Integer'
      end
    end
  end
end
