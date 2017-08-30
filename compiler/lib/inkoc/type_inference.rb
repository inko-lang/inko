# frozen_string_literal: true

module Inkoc
  class TypeInference
    def initialize(state)
      @state = state
    end

    def infer(node, self_type, mod)
      callback = node.tir_process_node_method

      public_send(callback, node, self_type, mod)
    end

    def on_integer(*)
      typedb.integer_type
    end

    def on_float(*)
      typedb.float_type
    end

    def on_string(*)
      typedb.string_type
    end

    def on_array(node, self_type, mod)
      type = type_of_global(Config::ARRAY_CONST, mod).new_instance

      unless node.values.empty?
        first_type = nil

        node.values.each do |value|
          value_type = infer(value, self_type, mod)

          unless first_type
            first_type = value_type
            next
          end

          next if value_type.type_compatible?(first_type)

          diagnostics.type_error(first_type, value_type, value.location)
        end

        type.init_type_parameter(type.type_parameter_names[0], first_type)
      end

      type
    end

    def on_hash_map(node, self_type, mod)
      type = type_of_global(Config::HASH_MAP_CONST, mod).new_instance

      unless node.pairs.empty?
        first_ktype = nil
        first_vtype = nil

        node.pairs.each do |(key, value)|
          ktype = infer(key, self_type, mod)
          vtype = infer(value, self_type, mod)

          if !first_ktype && !first_vtype
            first_ktype = ktype
            first_vtype = vtype
            next
          end

          unless ktype.type_compatible?(first_ktype)
            diagnostics.type_error(first_ktype, ktype, key.location)
          end

          unless vtype.type_compatible?(first_vtype)
            diagnostics.type_error(first_vtype, vtype, value.location)
          end
        end
      end

      type
    end

    def on_attribute(node, self_type, *)
      self_type.lookup_attribute(node.name).type
    end

    def on_identifier(node, self_type, mod)
      # TODO: need local variable access.
    end

    def on_send(node, self_type, mod)
      # TODO: implement
    end

    def typedb
      @state.typedb
    end

    def diagnostics
      @state.diagnostics
    end

    def type_of_global(name, mod)
      mod.globals[name].type
    end
  end
end
