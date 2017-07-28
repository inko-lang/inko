# frozen_string_literal: true

module Inkoc
  module Type
    class Dynamic < Object
      include Inspect

      def initialize
        super()
      end

      def block?
        false
      end

      # Dynamic types are compatible with everything else.
      def type_compatible?(*)
        true
      end
    end
  end
end
