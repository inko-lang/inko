# frozen_string_literal: true

module Inkoc
  module TypeSystem
    class Void
      include Type
      include NewInstance

      def void?
        true
      end

      def type_name
        Config::VOID_TYPE
      end

      def type_compatible?(_other, *)
        # Void is compatible with everything else because it never returns. This
        # allows one to write code such as:
        #
        #     try { foo } else { vm.panic('oops') }
        true
      end

      def type_instance?
        false
      end

      def new_instance(*)
        self
      end

      def lookup_method(name)
        NullSymbol.new(name)
      end
    end
  end
end
